use std::sync::Arc;

use bincode::serialize;
use crate::message::StreamletMessage;
use ed25519_dalek::{Keypair, Signer};
use log::{error, warn};
use rand::rngs::OsRng;
use rand::TryRngCore;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, error::TrySendError, Sender};

pub const NONCE_BYTES_LENGTH: usize = 32;
const OUTBOUND_CHANNEL_SIZE: usize = 64;


pub fn generate_nonce() -> [u8; NONCE_BYTES_LENGTH] {
    let mut nonce = [0u8; NONCE_BYTES_LENGTH];
    OsRng.try_fill_bytes(&mut nonce).expect("Error filling handshake nonce bytes");
    nonce
}

pub fn spawn_writer(mut stream: TcpStream) -> Sender<Arc<[u8]>> {
    let (sender, mut receiver) = mpsc::channel::<Arc<[u8]>>(OUTBOUND_CHANNEL_SIZE);
    tokio::spawn(async move {
        while let Some(buffer) = receiver.recv().await {
            if let Err(e) = stream.write_all(&buffer).await {
                error!("Failed to send message to connection: {}", e);
                break;
            }
        }
    });
    sender
}

pub fn broadcast(
    private_key: &Keypair,
    connections: &mut Vec<Option<Sender<Arc<[u8]>>>>,
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

    let shared_buffer: Arc<[u8]> = buffer.into();

    for (index, slot) in connections.iter_mut().enumerate() {
        let Some(sender) = slot else { continue };
        if sender.is_closed() {
            *slot = None;
            continue;
        }

        match sender.try_send(shared_buffer.clone()) {
            Ok(_) => {}
            Err(TrySendError::Full(_)) => {
                warn!("Outbound channel to connection {} is full; dropping message", index);
            }
            Err(TrySendError::Closed(_)) => {
                *slot = None;
                error!("Outbound channel to connection {} is closed", index);
            }
        }
    }
}
