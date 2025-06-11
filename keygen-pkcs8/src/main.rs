use base64::{engine::general_purpose, Engine as _};
use std::fs::File;
use std::io::{BufWriter, Write};
use ed25519_dalek::Keypair;
use rand::rngs::OsRng;

fn main() {
    let num_keys = 10;
    let mut csprng = OsRng{};
    let file = File::create("keys").expect("failed to create file");
    let mut writer = BufWriter::new(file);

    for _ in 0..num_keys {
        let keypair: Keypair = Keypair::generate(&mut csprng);
        let encoded_private = general_purpose::STANDARD.encode(keypair.to_bytes());
        let encoded_public = general_purpose::STANDARD.encode(keypair.public.to_bytes());

        writeln!(writer, "{}:{}", encoded_private, encoded_public).expect("failed to write to file");
    }

    writer.flush().expect("failed to flush file");
}
