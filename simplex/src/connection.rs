use crate::block::NodeId;
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_slice, to_string};
use shared::domain::node::Node;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::Sender;

const MESSAGE_BYTES_LENGTH: usize = 4;
const MESSAGE_SIGNATURE_BYTES_LENGTH: usize = 64;


pub async fn connect<M>(
    my_node_id: u32,
    nodes: &Vec<Node>,
    public_keys: &HashMap<u32, Vec<u8>>,
    sender: Arc<Sender<(NodeId, M)>>,
    listener: TcpListener,
) -> Vec<TcpStream> where M: DeserializeOwned + Send + 'static {
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

async fn handle_connection<M>(
    mut socket: TcpStream,
    sender: Arc<Sender<(NodeId, M)>>,
    public_keys: HashMap<u32, Vec<u8>>,
) where M: DeserializeOwned {
    'init: loop {
        let mut length_bytes = [0; MESSAGE_BYTES_LENGTH];
        match socket.read_exact(&mut length_bytes).await {
            Ok(_) => { }
            Err(e) => {
                eprintln!("Error reading length bytes: {}", e);
                return;
            }
        };
        let length = u32::from_be_bytes(length_bytes);
        let mut buffer = vec![0; length as usize];
        match socket.read_exact(&mut buffer).await {
            Ok(_) => { }
            Err(e) => {
                eprintln!("Error reading message bytes: {}", e);
                return;
            }
        };
        let mut signature = vec![0; MESSAGE_SIGNATURE_BYTES_LENGTH];
        match socket.read_exact(&mut signature).await {
            Ok(_) => { }
            Err(e) => {
                eprintln!("Error reading signature bytes: {}", e);
                return;
            }
        };

        for (id, key) in public_keys.iter() {
            let public_key = UnparsedPublicKey::new(&ED25519, key);
            match public_key.verify(&buffer, &signature) {
                Ok(_) => {
                    match from_slice::<M>(&buffer) {
                        Ok(message) => {
                            match sender.send((*id, message)).await {
                                Ok(_) => {
                                    continue 'init;
                                },
                                Err(_) => {
                                    println!("Error sending through channel");
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize message: {}", e);
                            return;
                        }
                    }
                },
                Err(_) => { continue }
            }
        }

        println!("Message signature verification failed.");
    }
}

pub async fn broadcast<M>(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<TcpStream>,
    message: M
) where M: Serialize {
    let serialized_message = match to_string(&message) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let serialized_bytes = serialized_message.as_bytes();
    let length = serialized_bytes.len() as u32;
    let length_bytes = length.to_be_bytes();
    let signature = private_key.sign(serialized_bytes);

    for i in 0..connections.len() {
        let stream = &mut connections[i];
        if let Err(e) = stream.write_all(&length_bytes).await {
            eprintln!("Failed to send message length to socket: {}", e);
            connections.remove(i);
            continue;
        }
        if let Err(e) = stream.write_all(serialized_bytes).await {
            eprintln!("Failed to send message to socket: {}", e);
            connections.remove(i);
            continue;
        }
        if let Err(e) = stream.write_all(signature.as_ref()).await {
            eprintln!("Failed to send signature to socket: {}", e);
            connections.remove(i);
            continue;
        }
    }
}

pub async fn broadcast_to_sample<M>(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<TcpStream>,
    message: M,
    sample_set: HashSet<u32>,
) where M: Serialize {
    let serialized_message = match to_string(&message) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let serialized_bytes = serialized_message.as_bytes();
    let length = serialized_bytes.len() as u32;
    let length_bytes = length.to_be_bytes();
    let signature = private_key.sign(serialized_bytes);

    for i in sample_set {
        let stream = &mut connections[i as usize];
        if let Err(e) = stream.write_all(&length_bytes).await {
            eprintln!("Failed to send message length to socket: {}", e);
            connections.remove(i as usize);
            continue;
        }
        if let Err(e) = stream.write_all(serialized_bytes).await {
            eprintln!("Failed to send message to socket: {}", e);
            connections.remove(i as usize);
            continue;
        }
        if let Err(e) = stream.write_all(signature.as_ref()).await {
            eprintln!("Failed to send signature to socket: {}", e);
            connections.remove(i as usize);
            continue;
        }
    }
}

pub async fn notify<M>(
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<TcpStream>,
    message: M,
    my_node_id: u32,
) where M: Serialize {
    let serialized_message = match to_string(&message) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let serialized_bytes = serialized_message.as_bytes();
    let length = serialized_bytes.len() as u32;
    let length_bytes = length.to_be_bytes();
    let signature = private_key.sign(serialized_bytes);

    for i in 0..connections.len() {
        let stream = &mut connections[i];
        if i != my_node_id as usize {
            if let Err(e) = stream.write_all(&length_bytes).await {
                eprintln!("Failed to send message length to socket: {}", e);
                connections.remove(i);
                continue;
            }
            if let Err(e) = stream.write_all(serialized_bytes).await {
                eprintln!("Failed to send message to socket: {}", e);
                connections.remove(i);
                continue;
            }
            if let Err(e) = stream.write_all(signature.as_ref()).await {
                eprintln!("Failed to send signature to socket: {}", e);
                connections.remove(i);
                continue;
            }
        }
    }
}

pub async fn unicast<M>(
    private_key: &Ed25519KeyPair,
    connection: &mut TcpStream,
    message: M
) where M: Serialize {
    let serialized_message = match to_string(&message) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let serialized_bytes = serialized_message.as_bytes();
    let length = serialized_bytes.len() as u32;
    let length_bytes = length.to_be_bytes();
    let signature = private_key.sign(serialized_bytes);

    if let Err(e) = connection.write_all(&length_bytes).await {
        eprintln!("Failed to send message length to socket: {}", e);
        return;
    }
    if let Err(e) = connection.write_all(serialized_bytes).await {
        eprintln!("Failed to send message to socket: {}", e);
        return;
    }
    if let Err(e) = connection.write_all(signature.as_ref()).await {
        eprintln!("Failed to send signature to socket: {}", e);
        return;
    }
}
