mod message;
mod block;
mod streamlet;
mod connection;

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
    match get_environment(args) {
        Ok(env) => {
            info!("Successfully read environment: {:?}", env);
            let public_keys = get_public_keys();
            let private_key = get_private_key(env.my_node.id);
            Streamlet::new(env, public_keys, private_key).start().await
        },
        Err(err) => {
            error!("Error: {}", err);
        }
    }
}
