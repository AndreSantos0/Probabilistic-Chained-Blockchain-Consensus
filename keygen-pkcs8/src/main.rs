use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use base64::{engine::general_purpose, Engine as _};
use std::fs::File;
use std::io::{BufWriter, Write};

fn main() {
    let num_keys = 4;
    let rng = SystemRandom::new();

    let file = File::create("keys").expect("failed to create file");
    let mut writer = BufWriter::new(file);

    for _ in 0..num_keys {
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).expect("keygen failed");
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();
        let public_key = key_pair.public_key().as_ref();

        let encoded_private = general_purpose::STANDARD.encode(pkcs8_bytes.as_ref());
        let encoded_public = general_purpose::STANDARD.encode(public_key);

        writeln!(writer, "{}:{}", encoded_private, encoded_public)
            .expect("failed to write to file");
    }

    writer.flush().expect("failed to flush file");
}
