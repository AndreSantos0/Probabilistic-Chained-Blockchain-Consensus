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


pub async fn connect(
    my_node_id: u32,
    nodes: &Vec<Node>,
    public_keys: &HashMap<u32, Vec<u8>>,
    sender: Arc<Sender<(NodeId, SimplexMessage)>>,
    listener: TcpListener,
) -> Vec<TcpStream> {
    let mut connections = Vec::new();
    for node in nodes.iter() {
        let address = format!("{}:{}", node.host, node.port);
        match TcpStream::connect(address).await {
            Ok(stream) => connections.push(stream),
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
                        handle_connection(stream, sender, keys).await
                    });
                }
            },
            Err(e) => eprintln!("[Node {}] Failed to accept a connection: {}", my_node_id, e)
        }
        accepted += 1;
    }

    connections
}

fn is_allowed(addr: SocketAddr) -> bool {
    addr.ip().is_loopback() //TODO: Discuss this
}

async fn handle_connection(
    mut socket: TcpStream,
    sender: Arc<Sender<(NodeId, SimplexMessage)>>,
    public_keys: HashMap<u32, Vec<u8>>,
) {
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

        let message = match from_slice::<SimplexMessage>(&buffer) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Failed to deserialize message: {}", e);
                continue;
            }
        };

        async fn verify_and_send(
            key: &UnparsedPublicKey<&Vec<u8>>,
            id: &u32,
            sender: &Sender<(NodeId, SimplexMessage)>,
            message: SimplexMessage,
            buffer: &[u8],
            signature: &[u8],
        ) -> Result<(), ()> {
            if key.verify(buffer, signature).is_ok() {
                if sender.send((*id, message)).await.is_err() {
                    eprintln!("Error sending through channel");
                    return Err(());
                }
                Ok(())
            } else {
                Err(())
            }
        }

        match message {
            SimplexMessage::Vote(vote) => {
                let serialized = match to_string(&vote.header) {
                    Ok(json) => json.into_bytes(),
                    Err(e) => {
                        eprintln!("Failed to serialize vote header: {}", e);
                        continue;
                    }
                };


                if public_key.is_none() {
                    for (i, k) in &public_keys {
                        let key = UnparsedPublicKey::new(&ED25519, k);
                        if verify_and_send(&key, i, &sender, SimplexMessage::Vote(vote.clone()), &serialized, vote.signature.as_ref()).await.is_ok() {
                            public_key = Some(key);
                            id = Some(i);
                            break;
                        }
                    }
                    continue;
                } else if let (Some(ref key), Some(node_id)) = (&public_key, id) {
                    let _ = verify_and_send(key, node_id, &sender, SimplexMessage::Vote(vote.clone()), &serialized, vote.signature.as_ref()).await;
                }
            }

            _ => {
                let mut signature = vec![0; MESSAGE_SIGNATURE_BYTES_LENGTH];
                if socket.read_exact(&mut signature).await.is_err() {
                    eprintln!("Error reading signature bytes");
                    return;
                }

                if public_key.is_none() {
                    for (i, k) in &public_keys {
                        let key = UnparsedPublicKey::new(&ED25519, k);
                        if verify_and_send(&key, i, &sender, message.clone(), &buffer, &signature).await.is_ok() {
                            public_key = Some(key);
                            id = Some(i);
                            break;
                        }
                    }
                    continue;
                }

                if let (Some(ref key), Some(node_id)) = (&public_key, id) {
                    let _ = verify_and_send(key, node_id, &sender, message, &buffer, &signature).await;
                }
            }
        }
    }
}

pub async fn broadcast(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<TcpStream>,
    message: SimplexMessage,
) {
    let serialized_bytes = match to_string(&message) {
        Ok(json) => json.into_bytes(),
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (serialized_bytes.len() as u32).to_be_bytes();
    let signature = match message {
        SimplexMessage::Vote(_) => None,
        _ => Some(private_key.sign(&serialized_bytes)),
    };


    let mut failed_indices = vec![];

    for (i, stream) in connections.iter_mut().enumerate() {
        if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&serialized_bytes).await.is_err() || match &signature {
            Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
            None => false,
        } {
            eprintln!("Failed to send message to connection {}", i);
            failed_indices.push(i);
        }
    }

    for i in failed_indices.into_iter().rev() {
        connections.remove(i);
    }
}

pub async fn broadcast_to_sample(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<TcpStream>,
    message: SimplexMessage,
    sample_set: Vec<u32>,
) {
    let serialized_bytes = match to_string(&message) {
        Ok(json) => json.into_bytes(),
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (serialized_bytes.len() as u32).to_be_bytes();
    let signature = match message {
        SimplexMessage::Vote(_) => None,
        _ => Some(private_key.sign(&serialized_bytes)),
    };

    for i in sample_set {
        let stream = &mut connections[i as usize];
        if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&serialized_bytes).await.is_err() || match &signature {
            Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
            None => false,
        } {
            eprintln!("Failed to send message to connection {}", i);
            connections.remove(i as usize);
        }
    }
}

pub async fn notify(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<TcpStream>,
    message: SimplexMessage,
    my_node_id: u32,
) {
    let serialized_bytes = match to_string(&message) {
        Ok(json) => json.into_bytes(),
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (serialized_bytes.len() as u32).to_be_bytes();
    let signature = match message {
        SimplexMessage::Vote(_) => None,
        _ => Some(private_key.sign(&serialized_bytes)),
    };


    let mut failed_indices = vec![];

    for (i, stream) in connections.iter_mut().enumerate() {
        if i == my_node_id as usize {
            continue
        }
        if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&serialized_bytes).await.is_err() || match &signature {
            Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
            None => false,
        } {
            eprintln!("Failed to send message to connection {}", i);
            failed_indices.push(i);
        }
    }

    for i in failed_indices.into_iter().rev() {
        connections.remove(i);
    }
}

pub async fn unicast(
    private_key: &Ed25519KeyPair,
    connection: &mut TcpStream,
    message: SimplexMessage
) {
    let serialized_bytes = match to_string(&message) {
        Ok(json) => json.into_bytes(),
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (serialized_bytes.len() as u32).to_be_bytes();
    let signature = match message {
        SimplexMessage::Vote(_) => None,
        _ => Some(private_key.sign(&serialized_bytes)),
    };

    if connection.write_all(&length_bytes).await.is_err() || connection.write_all(&serialized_bytes).await.is_err() || match &signature {
        Some(sig) => connection.write_all(sig.as_ref()).await.is_err(),
        None => false,
    } {
        eprintln!("Failed to send message to connection");
    }
}
