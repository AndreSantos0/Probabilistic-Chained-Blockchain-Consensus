use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde::{Serialize};
use serde::de::DeserializeOwned;
use serde_json::{from_slice, to_string};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::Sender;
use crate::domain::node::Node;

const MESSAGE_BYTES_LENGTH: usize = 4;
const MESSAGE_SIGNATURE_BYTES_LENGTH: usize = 64;

pub type NodeId = u32;
pub type Signature = Vec<u8>;

pub async fn connect<M>(
    my_node_id: u32,
    nodes: &Vec<Node>,
    public_keys: &HashMap<u32, Vec<u8>>,
    sender: Arc<Sender<(NodeId, M, Signature)>>
) where M: DeserializeOwned + Send + 'static {
    for node in nodes.iter() {
        let address = format!("{}:{}", node.host, node.port);
        let node_id = node.id;
        match TcpStream::connect(address).await {
            Ok(stream) => {
                let sender = Arc::clone(&sender);
                if let Some(public_key) = public_keys.get(&node.id).cloned() {
                    tokio::spawn(async move {
                        handle_connection(stream, sender, &public_key, node_id).await
                    });
                }
            }
            Err(e) => eprintln!("[Node {}] Failed to connect to Node {}: {}", my_node_id, node.id, e),
        }
    }
}

pub async fn accept_connections(my_node_id: u32, nodes: &Vec<Node>, listener: TcpListener) -> Vec<TcpStream> {
    let mut accepted = 0;
    let mut connections = Vec::new();
    let n_nodes = nodes.len();
    while accepted < n_nodes {
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                if is_allowed(addr) {
                    connections.push(stream);
                } else {
                    println!("[Node {}] Rejected connection from host {}, port {}", my_node_id, addr.ip().to_string(), addr.port());
                    if let Err(e) = stream.shutdown().await {
                        eprintln!("[Node {}] Failed to close connection from host {}, port {}: {}", my_node_id, addr.ip().to_string(), addr.port(), e);
                    }
                    continue
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
    sender: Arc<Sender<(NodeId, M, Signature)>>,
    public_key: &Vec<u8>,
    node_id: u32
) where M: DeserializeOwned {
    let public_key = UnparsedPublicKey::new(&ED25519, public_key);
    loop {
        let mut length_bytes = [0; MESSAGE_BYTES_LENGTH];
        match socket.read_exact(&mut length_bytes).await {
            Ok(_) => {  }
            Err(e) => {
                println!("Error reading length bytes: {}", e);
                return;
            }
        };
        let length = u32::from_be_bytes(length_bytes);
        let mut buffer = vec![0; length as usize];
        match socket.read_exact(&mut buffer).await {
            Ok(_) => {  }
            Err(e) => {
                println!("Error reading message bytes: {}", e);
                return;
            }
        };
        let mut signature = vec![0; MESSAGE_SIGNATURE_BYTES_LENGTH];
        match socket.read_exact(&mut signature).await {
            Ok(_) => {  }
            Err(e) => {
                println!("Error reading signature bytes: {}", e);
                return;
            }
        };
        match public_key.verify(&buffer, &signature) {
            Ok(_) => {
                match from_slice::<M>(&buffer) {
                    Ok(message) => {
                        match sender.send((node_id, message, signature)).await {
                            Ok(_) => {  },
                            Err(_) => return
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize message: {}", e);
                        return;
                    }
                }
            },
            Err(_) => {
                println!("Signature verification failed.");
                return;
            }
        }
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
        println!("Sent message {}", serialized_message);
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
    println!("Sent message {}", serialized_message);
}
