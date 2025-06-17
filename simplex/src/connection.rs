use bincode::{serialize};
use ed25519_dalek::{Keypair, Signer};
use crate::message::SimplexMessage;
use log::{error};
use rand::rngs::OsRng;
use rand::TryRngCore;
use tokio::io::{AsyncWriteExt};
use tokio::net::{TcpStream};

pub const NONCE_BYTES_LENGTH: usize = 32;
pub const MESSAGE_BYTES_LENGTH: usize = 4;
pub const SIGNATURE_BYTES_LENGTH: usize = 64;


pub fn generate_nonce() -> [u8; NONCE_BYTES_LENGTH] {
    let mut nonce = [0u8; NONCE_BYTES_LENGTH];
    OsRng.try_fill_bytes(&mut nonce).expect("Error filling handshake nonce bytes");
    nonce
}

pub async fn broadcast<M: SimplexMessage>(
    private_key: &Keypair,
    connections: &mut Vec<Option<TcpStream>>,
    message: &M,
    enable_crypto: bool,
) {
    let payload = match serialize(message) {
        Ok(msg) => msg,
        Err(e) => {
            error!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (payload.len() as u32).to_be_bytes();
    let signature = if message.is_vote() || !enable_crypto {
        None
    } else {
        Some(private_key.sign(&payload))
    };

    let mut buffer = Vec::with_capacity(length_bytes.len() + payload.len() + signature.as_ref().map_or(0, |s| s.as_ref().len()));
    buffer.extend(&length_bytes);
    buffer.extend(&payload);
    if let Some(sig) = &signature {
        buffer.extend(sig.as_ref());
    }

    let mut failed_indices = vec![];

    for (i, stream) in connections.iter_mut().enumerate() {
        if let Some(stream) = stream {
            if let Err(e) = stream.write_all(&buffer).await {
                error!("Failed to send message to connection {}: {}", i, e);
                failed_indices.push(i);
            }
        }
    }

    for i in failed_indices.into_iter().rev() {
        connections[i] = None;
    }
}

pub async fn broadcast_to_sample<M: SimplexMessage>(
    private_key: &Keypair,
    connections: &mut Vec<Option<TcpStream>>,
    message: &M,
    my_node_id: u32,
    enable_crypto: bool,
) {
    let payload = match serialize(message) {
        Ok(msg) => msg,
        Err(e) => {
            error!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (payload.len() as u32).to_be_bytes();
    let signature = if message.is_vote() || !enable_crypto {
        None
    } else {
        Some(private_key.sign(&payload))
    };

    let mut failed_indices = vec![];
    if let Some(sample_set) = message.get_sample_set() {
        for i in sample_set {
            if *i != my_node_id {
                let id = if my_node_id >= *i {
                    *i
                } else {
                    *i - 1
                };
                let stream = &mut connections[id as usize];
                match stream {
                    None => {}
                    Some(stream) => {
                        if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&payload).await.is_err() || match &signature {
                            Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
                            None => false,
                        } {
                            error!("Failed to send message to connection {}", i);
                            failed_indices.push(id);
                        }
                    }
                }
            }
        }

        for i in failed_indices.into_iter().rev() {
            connections[i as usize] = None;
        }
    }
}

pub async fn unicast<M: SimplexMessage>(
    private_key: &Keypair,
    connections: &mut Vec<Option<TcpStream>>,
    message: &M,
    recipient: u32,
    my_node_id: u32,
    enable_crypto: bool,
) {
    let payload = match serialize(message) {
        Ok(msg) => msg,
        Err(e) => {
            error!("Failed to serialize message: {}", e);
            return;
        }
    };

    let length_bytes = (payload.len() as u32).to_be_bytes();
    let signature = if message.is_vote() || !enable_crypto {
        None
    } else {
        Some(private_key.sign(&payload))
    };

    let id = if my_node_id >= recipient {
        recipient
    } else {
        recipient - 1
    };

    let mut buffer = Vec::with_capacity(length_bytes.len() + payload.len() + signature.as_ref().map_or(0, |s| s.as_ref().len()));
    buffer.extend(length_bytes);
    buffer.extend(payload);
    if let Some(sig) = &signature {
        buffer.extend(sig.as_ref());
    }

    let stream = &mut connections[id as usize];
    match stream {
        None => {}
        Some(connection) => {
            if let Err(e) = connection.write_all(&buffer).await {
                error!("Failed to send message part to connection {}: {}", recipient, e);
            }
        }
    }
}