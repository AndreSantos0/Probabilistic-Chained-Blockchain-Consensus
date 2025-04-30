use crate::block::NodeId;
use crate::message::SimplexMessage;
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde_json::{from_slice, to_string};
use shared::domain::node::Node;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::Sender;

const MESSAGE_BYTES_LENGTH: usize = 4;
const MESSAGE_SIGNATURE_BYTES_LENGTH: usize = 64;
const DEFAULT_SENDER: u32 = 0;


pub async fn connect<M: SimplexMessage>(
    my_node_id: u32,
    nodes: &Vec<Node>,
    public_keys: &HashMap<u32, Vec<u8>>,
    sender: Arc<Sender<(NodeId, M)>>,
    listener: TcpListener,
    enable_crypto: bool,
) -> Vec<Option<TcpStream>>
where
    M: SimplexMessage + 'static
{
    let mut connections = Vec::new();
    for node in nodes.iter() {
        let address = format!("{}:{}", node.host, node.port);
        match TcpStream::connect(address).await {
            Ok(stream) => {
                connections.push(Some(stream));
            }
            Err(e) => eprintln!("[Node {}] Failed to connect to Node {}: {}", my_node_id, node.id, e),
        }
    }

    let mut accepted = 0;
    while accepted != nodes.len() {
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                if !is_allowed(addr) {
                    println!("[Node {}] Rejected connection from host {}, port {}", my_node_id, addr.ip().to_string(), addr.port());
                    if let Err(e) = stream.shutdown().await {
                        eprintln!("[Node {}] Failed to close connection from host {}, port {}: {}", my_node_id, addr.ip().to_string(), addr.port(), e);
                    }
                } else {
                    let sender = Arc::clone(&sender);
                    let keys = public_keys.clone();
                    tokio::spawn(async move {
                        handle_connection(stream, sender, keys, enable_crypto).await
                    });
                }
            },
            Err(e) => eprintln!("[Node {}] Failed to accept a connection: {}", my_node_id, e)
        }
        accepted += 1;
    }

    connections
}

fn is_allowed(_addr: SocketAddr) -> bool {
    true
}

async fn handle_connection<M>(
    mut socket: TcpStream,
    sender: Arc<Sender<(NodeId, M)>>,
    public_keys: HashMap<u32, Vec<u8>>,
    enable_crypto: bool,
)
where
    M: SimplexMessage + Clone + 'static,
{
    let mut public_key: Option<UnparsedPublicKey<&Vec<u8>>> = None;
    let mut id: Option<&u32> = None;

    loop {
        let mut length_bytes = [0; MESSAGE_BYTES_LENGTH];
        if socket.read_exact(&mut length_bytes).await.is_err() {
            eprintln!("Error reading length bytes");
            return;
        }

        let length = u32::from_be_bytes(length_bytes);
        let mut buffer = vec![0; length as usize];
        if socket.read_exact(&mut buffer).await.is_err() {
            eprintln!("Error reading message bytes");
            return;
        }

        let message: M = match from_slice(&buffer) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Failed to deserialize message: {}", e);
                return;
            }
        };

        async fn verify_and_send<M: SimplexMessage>(
            key: &UnparsedPublicKey<&Vec<u8>>,
            id: &u32,
            sender: &Sender<(NodeId, M)>,
            message: M,
            buffer: &[u8],
            signature: &[u8],
        ) -> Result<(), ()> {
            if key.verify(buffer, signature).is_ok() {
                let _ = sender.send((*id, message)).await;
                Ok(())
            } else {
                Err(())
            }
        }

        if enable_crypto {
            let mut signature = vec![0; MESSAGE_SIGNATURE_BYTES_LENGTH];
            let (payload, signature) = if message.is_vote() {
                (message.get_vote_header_bytes().unwrap_or_default(), message.get_signature_bytes().unwrap_or_default())
            } else {
                if socket.read_exact(&mut signature).await.is_err() {
                    eprintln!("Error reading signature bytes");
                    return;
                }
                (buffer, signature.as_ref())
            };

            if public_key.is_none() {
                for (i, k) in &public_keys {
                    let key = UnparsedPublicKey::new(&ED25519, k);
                    if verify_and_send(&key, i, &sender, message.clone(), &payload, signature).await.is_ok() {
                        public_key = Some(key);
                        id = Some(i);
                        break;
                    }
                }
                continue;
            }

            if let (Some(ref key), Some(node_id)) = (&public_key, id) {
                let _ = verify_and_send(key, node_id, &sender, message.clone(), &payload, signature).await;
            }
        } else {
            let _ = sender.send((DEFAULT_SENDER, message)).await;
        }
    }
}

pub async fn broadcast<M: SimplexMessage>(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<Option<TcpStream>>,
    message: M,
    enable_crypto: bool,
) {
    let serialized_bytes = match to_string(&message) {
        Ok(json) => json.into_bytes(),
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (serialized_bytes.len() as u32).to_be_bytes();
    let signature = if message.is_vote() || !enable_crypto {
        None
    } else {
        Some(private_key.sign(&serialized_bytes))
    };

    let mut failed_indices = vec![];

    for (i, stream) in connections.iter_mut().enumerate() {
        match stream {
            None => {}
            Some(stream) => {
                if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&serialized_bytes).await.is_err() || match &signature {
                    Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
                    None => false,
                } {
                    eprintln!("Failed to send message to connection {}", i);
                    failed_indices.push(i);
                }
            }
        }
    }

    for i in failed_indices.into_iter().rev() {
        connections[i] = None;
    }
}

pub async fn broadcast_to_sample<M: SimplexMessage>(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<Option<TcpStream>>,
    message: M,
    sample_set: Vec<u32>,
    enable_crypto: bool,
) {
    let serialized_bytes = match to_string(&message) {
        Ok(json) => json.into_bytes(),
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (serialized_bytes.len() as u32).to_be_bytes();
    let signature = if message.is_vote() || !enable_crypto {
        None
    } else {
        Some(private_key.sign(&serialized_bytes))
    };

    let mut failed_indices = vec![];

    for i in sample_set {
        let stream = &mut connections[i as usize];
        match stream {
            None => {}
            Some(stream) => {
                if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&serialized_bytes).await.is_err() || match &signature {
                    Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
                    None => false,
                } {
                    eprintln!("Failed to send message to connection {}", i);
                    failed_indices.push(i);
                }
            }
        }
    }

    for i in failed_indices.into_iter().rev() {
        connections[i as usize] = None;
    }
}

pub async fn notify<M: SimplexMessage>(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<Option<TcpStream>>,
    message: M,
    my_node_id: u32,
    enable_crypto: bool,
) {
    let serialized_bytes = match to_string(&message) {
        Ok(json) => json.into_bytes(),
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (serialized_bytes.len() as u32).to_be_bytes();
    let signature = if message.is_vote() || !enable_crypto {
        None
    } else {
        Some(private_key.sign(&serialized_bytes))
    };

    let mut failed_indices = vec![];

    for (i, stream) in connections.iter_mut().enumerate() {
        if i == my_node_id as usize {
            continue
        }
        match stream {
            None => {}
            Some(stream) => {
                if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&serialized_bytes).await.is_err() || match &signature {
                    Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
                    None => false,
                } {
                    eprintln!("Failed to send message to connection {}", i);
                    failed_indices.push(i);
                }
            }
        }
    }

    for i in failed_indices.into_iter().rev() {
        connections[i] = None;
    }
}

pub async fn unicast<M: SimplexMessage>(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<Option<TcpStream>>,
    message: M,
    recipient: u32,
    enable_crypto: bool,
) {
    let serialized_bytes = match to_string(&message) {
        Ok(json) => json.into_bytes(),
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (serialized_bytes.len() as u32).to_be_bytes();
    let signature = if message.is_vote() || !enable_crypto {
        None
    } else {
        Some(private_key.sign(&serialized_bytes))
    };

    let stream = &mut connections[recipient as usize];
    match stream {
        None => {}
        Some(connection) => {
            if connection.write_all(&length_bytes).await.is_err() || connection.write_all(&serialized_bytes).await.is_err() || match &signature {
                Some(sig) => connection.write_all(sig.as_ref()).await.is_err(),
                None => false,
            } {
                eprintln!("Failed to send message to connection");
                connections[recipient as usize] = None
            }
        }
    }
}
