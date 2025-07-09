use bincode::{serialize};
use crate::message::{StreamletMessage};
use ed25519_dalek::{Keypair, Signer};
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

pub async fn broadcast(
    private_key: &Keypair,
    connections: &mut Vec<Option<TcpStream>>,
    message: &StreamletMessage,
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
    let signature = if !enable_crypto || message.is_echo() {
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
