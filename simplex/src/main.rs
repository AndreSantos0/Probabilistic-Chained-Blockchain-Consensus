mod message;
mod block;
mod connection;
mod protocol;
mod probabilistic_simplex;
mod practical_simplex;

use std::env;
use env_logger::Env;
use log::{error, info};
use shared::initializer::{get_environment, get_private_key, get_public_keys};
use crate::practical_simplex::PracticalSimplex;
use crate::probabilistic_simplex::ProbabilisticSimplex;
use crate::protocol::{Protocol, ProtocolMode};


#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let enable_logging = true;
    let args: Vec<String> = env::args().collect();
    if enable_logging {
        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    } else {
        log::set_max_level(log::LevelFilter::Off);
    }
    let protocol_mode = if args.iter().any(|arg| arg == "probabilistic") {
        ProtocolMode::Probabilistic
    } else {
        ProtocolMode::Practical
    };
    match get_environment(args) {
        Ok(env) => {
            info!("Successfully read environment: {:?}", env);
            //if env.my_node.id == 0 {
            //   console_subscriber::init();
            //}
            // let main = tokio::spawn(async move {
                let public_keys = get_public_keys();
                let private_key = get_private_key(env.my_node.id);
                match protocol_mode {
                    ProtocolMode::Practical => PracticalSimplex::new(env, public_keys, private_key).start().await,
                    ProtocolMode::Probabilistic => ProbabilisticSimplex::new(env, public_keys, private_key).start().await,
                };
            // });
            // main.await.expect("");
        },
        Err(err) => {
            error!("Error: {}", err);
        }
    }
}
