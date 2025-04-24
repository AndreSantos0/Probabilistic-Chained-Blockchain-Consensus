use ring::signature::{Ed25519KeyPair, KeyPair};
use ring::rand::SystemRandom;
use base64::{engine::general_purpose, Engine};

fn main() {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).expect("keygen failed");

    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();
    let public_key = key_pair.public_key().as_ref();

    let encoded_private = general_purpose::STANDARD.encode(pkcs8_bytes.as_ref());
    let encoded_public = general_purpose::STANDARD.encode(public_key);

    println!("{}:{}", encoded_private, encoded_public);
}
