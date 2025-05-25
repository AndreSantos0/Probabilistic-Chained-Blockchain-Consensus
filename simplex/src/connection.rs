use bincode::{serialize};
use crate::message::SimplexMessage;
use ring::signature::{Ed25519KeyPair};
use log::{error, info};
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
    private_key: &Ed25519KeyPair,
    connections: &mut Vec<Option<TcpStream>>,
    message: M,
    enable_crypto: bool,
) {
    let payload = match serialize(&message) {
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

    for (i, stream) in connections.iter_mut().enumerate() {
        match stream {
            None => {}
            Some(stream) => {
                if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&payload).await.is_err() || match &signature {
                    Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
                    None => false,
                } {
                    error!("Failed to send message to connection {}", i);
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
    let payload = match serialize(&message) {
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

    for i in sample_set {
        let stream = &mut connections[i as usize];
        match stream {
            None => {}
            Some(stream) => {
                if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&payload).await.is_err() || match &signature {
                    Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
                    None => false,
                } {
                    error!("Failed to send message to connection {}", i);
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
    let payload = match serialize(&message) {
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

    for (i, stream) in connections.iter_mut().enumerate() {
        if i == my_node_id as usize {
            continue
        }
        match stream {
            None => {}
            Some(stream) => {
                if stream.write_all(&length_bytes).await.is_err() || stream.write_all(&payload).await.is_err() || match &signature {
                    Some(sig) => stream.write_all(sig.as_ref()).await.is_err(),
                    None => false,
                } {
                    error!("Failed to send message to connection {}", i);
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
    let payload = match serialize(&message) {
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

    let stream = &mut connections[recipient as usize];
    match stream {
        None => {}
        Some(connection) => {
            if connection.write_all(&length_bytes).await.is_err() || connection.write_all(&payload).await.is_err() || match &signature {
                Some(sig) => connection.write_all(sig.as_ref()).await.is_err(),
                None => false,
            } {
                error!("Failed to send message to connection");
                connections[recipient as usize] = None
            }
        }
    }
}
