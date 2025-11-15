mod message;
mod block;
mod streamlet;
mod connection;
mod blockchain;

use std::env;
use env_logger::Env;
use log::{error, info};
use shared::initializer::{get_environment, get_private_key, get_public_keys};
use crate::streamlet::Streamlet;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let enable_logging = false;
    let args: Vec<String> = env::args().collect();
    if enable_logging {
        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    } else {
        log::set_max_level(log::LevelFilter::Off);
    }
    const EPOCH_ARG_INDEX: usize = 4;
    if args.len() <= EPOCH_ARG_INDEX {
        eprintln!("Usage: streamlet <node_id> <transaction_size> <n_transactions> <epoch_time> [options]");
        std::process::exit(1);
    }

    let epoch_time = args[EPOCH_ARG_INDEX].parse::<f64>().unwrap_or_else(|err| {
        eprintln!("Failed to parse epoch_time ({}): {}", args[EPOCH_ARG_INDEX], err);
        std::process::exit(1);
    });
    match get_environment(args) {
        Ok(env) => {
            info!("Successfully read environment: {:?}", env);

            let public_keys = get_public_keys();
            let private_key = get_private_key(env.my_node.id);
            Streamlet::new(env, epoch_time, public_keys, private_key).start().await
        },
        Err(err) => {
            error!("Error: {}", err);
        }
    }
}
