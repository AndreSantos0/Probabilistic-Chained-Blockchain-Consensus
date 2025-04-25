mod blockchain;
mod message;
mod block;
mod connection;
mod protocol;
mod probabilistic_simplex;
mod practical_simplex;

use std::env;
use chrono::{Local, Timelike};
use shared::initializer::{get_environment, get_private_key, get_public_keys, wait_until_specific_time, ADD_ONE_HOUR, ADD_ONE_MINUTE, MINUTES_PER_HOUR};
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
            let now = Local::now();
            let mut next_hour = now.hour();
            let mut minute = now.minute() + ADD_ONE_MINUTE;
            if minute >= MINUTES_PER_HOUR {
                next_hour += ADD_ONE_HOUR;
                minute -= MINUTES_PER_HOUR;
            }
            println!("Starting at {}:{}", next_hour, minute);
            //wait_until_specific_time(next_hour, minute).await;
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
