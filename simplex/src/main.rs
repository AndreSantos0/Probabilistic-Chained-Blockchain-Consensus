mod node;
mod blockchain;
mod message;
mod block;
mod connection;

use std::env;
use chrono::{Local, Timelike};
use shared::initializer::{get_environment, get_private_key, get_public_keys, wait_until_specific_time, ADD_ONE_HOUR, ADD_ONE_MINUTE, MINUTES_PER_HOUR};
use crate::node::SimplexNode;


#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    match get_environment(args) {
        Ok(env) => {
            println!("Successfully read environment: {:?}", env);

            let public_keys = get_public_keys();
            let private_key = get_private_key(env.my_node.id);
            let my_node = SimplexNode::new(env, public_keys, private_key);

            let now = Local::now();
            let mut next_hour = now.hour();
            let mut minute = now.minute() + ADD_ONE_MINUTE;

            if minute >= MINUTES_PER_HOUR {
                next_hour += ADD_ONE_HOUR;
                minute -= MINUTES_PER_HOUR;
            }

            println!("Starting at {}:{}", next_hour, minute);
            wait_until_specific_time(next_hour, minute).await;
            my_node.start_simplex().await;
        },
        Err(err) => {
            eprintln!("Error: {}", err);
        }
    }
}
