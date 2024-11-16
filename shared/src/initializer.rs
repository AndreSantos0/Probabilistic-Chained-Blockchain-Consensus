use std::collections::HashMap;
use std::error::Error;
use std::{env, fs};
use std::fs::File;
use chrono::{Local, Timelike};
use csv::ReaderBuilder;
use toml::Value;
use base64::{engine::general_purpose, Engine as _};
use ring::signature::Ed25519KeyPair;
use tokio::time::{sleep, Duration};
use crate::domain::environment::Environment;
use crate::domain::node::Node;

pub const MINUTES_PER_HOUR: u32 = 60;
pub const SECONDS_PER_HOUR: u32 = 3600;
pub const SECONDS_PER_MINUTE: u32 = 60;
pub const ADD_ONE_MINUTE: u32 = 1;
pub const ADD_ONE_HOUR: u32 = 1;
pub const N_PROGRAM_ARGS: usize = 3;
pub const MY_NODE_ID_ARG_POS: usize = 1;
pub const NODES_FILENAME_ARG_POS: usize = 2;
pub const PUBLIC_KEYS_FILENAME: &str = "./shared/public_keys.toml";
pub const PUBLIC_KEYS_FILE_INDEX: &str = "public_key";
pub const PRIVATE_KEY_ENV: &str = "PRIVATE_KEY_";


pub fn get_environment(args: Vec<String>) -> Result<Environment, Box<dyn Error>> {
    if args.len() < N_PROGRAM_ARGS {
        return Err("Specify Node Id and nodes´ CSV file [Streamlet_Rust.exe 1 nodes.csv]".into());
    }

    let my_id = args[MY_NODE_ID_ARG_POS].parse::<u32>()?;
    let file_path = &args[NODES_FILENAME_ARG_POS];
    let nodes = read_nodes_from_csv(file_path)?;

    let my_node = nodes.iter().find(|node| node.id == my_id)
        .ok_or("This process' node was not found")?
        .clone();

    Ok(Environment { my_node, nodes })
}

pub fn read_nodes_from_csv(file_path: &str) -> Result<Vec<Node>, Box<dyn Error>> {
    let file = File::open(file_path)?;
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file);

    let mut nodes = Vec::new();
    for result in rdr.deserialize() {
        match result {
            Ok(node) => nodes.push(node),
            Err(e) => eprintln!("Error parsing CSV line: {}", e),
        }
    }
    Ok(nodes)
}

pub fn get_public_keys() -> HashMap<u32, Vec<u8>> {
    let content = fs::read_to_string(PUBLIC_KEYS_FILENAME).expect("Failed to read public key file");
    let data: Value = content.parse::<Value>().expect("Failed to parse TOML data");
    let data_table = data.as_table().expect("Expected TOML data to be a table");
    let mut public_keys = HashMap::new();
    for (node_id, node_info) in data_table {
        if let Some(public_key_str) = node_info.get(PUBLIC_KEYS_FILE_INDEX).and_then(|v| v.as_str()) {
            let id = node_id.parse::<u32>().expect("Failed to parse node id from public key file");
            let public_key_bytes = general_purpose::STANDARD.decode(public_key_str).expect("Failed to decode base64 public key");
            public_keys.insert(id, public_key_bytes);
        }
    }
    public_keys
}

pub fn get_private_key(node_id: u32) -> Ed25519KeyPair {
    let encoded_key = env::var(format!("{}{}", PRIVATE_KEY_ENV, node_id)).expect("Private key environment variable is not set");
    let key_data = general_purpose::STANDARD.decode(encoded_key).expect("Failed to decode base64 private key");
    Ed25519KeyPair::from_pkcs8(&key_data).expect("Failed to parse private key")
}

pub async fn wait_until_specific_time(hour: u32, minute: u32) {
    let now = Local::now();
    let current_hour = now.hour();
    let current_minute = now.minute();
    let current_second = now.second();

    let target_hour = hour;
    let target_minute = minute;

    let hours_left = target_hour - current_hour;
    let minutes_left = target_minute - current_minute;
    let total_seconds_left = hours_left * SECONDS_PER_HOUR + minutes_left * SECONDS_PER_MINUTE - current_second;

    if total_seconds_left > 0 {
        let duration = Duration::from_secs(total_seconds_left as u64);
        sleep(duration).await;
    }
}
