mod blockchain;
mod message;
mod block;
mod connection;
mod protocol;
mod probabilistic_simplex;
mod practical_simplex;

use std::env;
use shared::initializer::{get_environment, get_private_key, get_public_keys};
use crate::practical_simplex::PracticalSimplex;
use crate::probabilistic_simplex::ProbabilisticSimplex;
use crate::protocol::{Protocol, ProtocolMode};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let protocol_mode = if args.iter().any(|arg| arg == "probabilistic") {
        ProtocolMode::Probabilistic
    } else {
        ProtocolMode::Practical
    };
    match get_environment(args) {
        Ok(env) => {
            println!("Successfully read environment: {:?}", env);
            let public_keys = get_public_keys();
            let private_key = get_private_key(env.my_node.id);
            match protocol_mode {
                ProtocolMode::Practical => PracticalSimplex::new(env, public_keys, private_key).start().await,
                ProtocolMode::Probabilistic => ProbabilisticSimplex::new(env, public_keys, private_key).start().await,
            };
        },
        Err(err) => {
            eprintln!("Error: {}", err);
        }
    }
}
