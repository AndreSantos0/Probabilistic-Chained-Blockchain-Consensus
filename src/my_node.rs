use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rand::prelude::StdRng;
use rand::{Rng, SeedableRng};
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde_json::{from_slice, to_string};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time;

use crate::blockchain::Blockchain;
use crate::domain::environment::Environment;
use crate::domain::message::{Message, Propose, Vote};
use crate::transaction_generator::TransactionGenerator;

const SEED: u64 = 0xFF6DA736EA;
const INITIAL_EPOCH: u32 = 0;
const EPOCH_TIME: u64 = 3;
const CONFUSION_START: u32 = 0;
const CONFUSION_DURATION: u32 = 0;
const MESSAGE_BYTES_LENGTH: usize = 4;
const MESSAGE_CHANNEL_SIZE: usize = 50;
const MESSAGE_SIGNATURE_BYTES_LENGTH: usize = 64;


pub struct MyNode {
    environment: Environment,
    epoch: Arc<AtomicU32>,
    is_new_epoch: Arc<AtomicBool>,
    blockchain: Blockchain,
    transaction_generator: TransactionGenerator,
    blocks: HashMap<u32, bool>,
    votes_ids: HashMap<u32, Vec<u32>>,
    public_keys: HashMap<u32, Vec<u8>>,
    private_key: Ed25519KeyPair,
}

impl MyNode {

    pub fn new(environment: Environment, public_keys: HashMap<u32, Vec<u8>>, private_key: Ed25519KeyPair) -> Self {
        let my_node_id = environment.my_node.id;
        MyNode {
            environment,
            epoch: Arc::new(AtomicU32::new(INITIAL_EPOCH)),
            is_new_epoch: Arc::new(AtomicBool::new(false)),
            blockchain: Blockchain::new(my_node_id),
            transaction_generator: TransactionGenerator::new(),
            blocks: HashMap::new(),
            votes_ids: HashMap::new(),
            public_keys,
            private_key
        }
    }

    async fn connect(&self, sender: Arc<Sender<Message>>) {
        for node in self.environment.nodes.iter() {
            let address = format!("{}:{}", node.host, node.port);
            match TcpStream::connect(address).await {
                Ok(stream) => {
                    let sender = Arc::clone(&sender);
                    if let Some(public_key) = self.public_keys.get(&node.id).cloned() {
                        tokio::spawn(async move {
                            handle_connection(stream, sender, &public_key).await
                        });
                    }
                }
                Err(e) => eprintln!("[Node {}] Failed to connect to Node {}: {}", self.environment.my_node.id, node.id, e),
            }
        }
    }

    async fn accept_connections(&self, listener: TcpListener) -> Vec<TcpStream> {
        let mut accepted = 0;
        let mut connections = Vec::new();
        let n_nodes = self.environment.nodes.len();
        while accepted < n_nodes {
            match listener.accept().await {
                Ok((mut stream, addr)) => {
                    if self.is_allowed(addr) {
                        connections.push(stream);
                    } else {
                        println!("[Node {}] Rejected connection from host {}, port {}", self.environment.my_node.id, addr.ip().to_string(), addr.port());
                        if let Err(e) = stream.shutdown().await {
                            eprintln!("[Node {}] Failed to close connection from host {}, port {}: {}", self.environment.my_node.id, addr.ip().to_string(), addr.port(), e);
                        }
                        continue
                    }
                },
                Err(e) => eprintln!("[Node {}] Failed to accept a connection: {}", self.environment.my_node.id, e)
            }
            accepted += 1;
        }
        connections
    }

    fn is_allowed(&self, addr: SocketAddr) -> bool {
        addr.ip().is_loopback() //TODO: Discuss this
    }

    pub async fn start_streamlet(mut self) {
        let address = format!("{}:{}", self.environment.my_node.host, self.environment.my_node.port);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let (tx, rx) = mpsc::channel::<Message>(MESSAGE_CHANNEL_SIZE);
                let sender = Arc::new(tx);
                self.connect(sender).await;
                let mut connections = self.accept_connections(listener).await;
                self.start_epoch_counter();
                self.execute_protocol(rx, &mut connections).await;
            }
            Err(_) => { eprintln!("[Node {}] Failed to bind local port", self.environment.my_node.id) }
        };
    }

    fn start_epoch_counter(&self) {
        let epoch_counter = self.epoch.clone();
        let is_new_epoch = self.is_new_epoch.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(EPOCH_TIME));
            loop {
                interval.tick().await;
                epoch_counter.fetch_add(1, Ordering::SeqCst);
                loop {
                    match is_new_epoch.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
                        Ok(_) => break,
                        Err(_) => {}
                    }
                }
            }
        });
    }

    async fn execute_protocol(&mut self, mut message_queue_receiver: Receiver<Message>, connections: &mut Vec<TcpStream>) {
        let mut rng = StdRng::seed_from_u64(SEED);
        loop {
            match self.is_new_epoch.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => {
                    let epoch = self.epoch.load(Ordering::SeqCst);
                    let leader = self.get_leader(epoch, &mut rng);
                    println!("----------------------");
                    println!("----------------------");
                    println!("Leader is node {} | Epoch: {}", leader, epoch);
                    self.blockchain.print();
                    if leader == self.environment.my_node.id {
                        self.propose(connections).await;
                    }
                }
                Err(_) => {}
            }

            let epoch = self.epoch.load(Ordering::SeqCst);
            if epoch < CONFUSION_START || epoch >= CONFUSION_START + CONFUSION_DURATION {
                match message_queue_receiver.try_recv() {
                    Ok(msg) => {
                        match msg {
                            Message::Propose(propose) => {
                                if !self.blocks.contains_key(&propose.content.epoch) {
                                    self.blocks.insert(propose.content.epoch, true);
                                    let message = Message::Propose(Propose { content: propose.content.clone(), sender: propose.sender });
                                    self.broadcast(connections, message).await;
                                    let length = self.blockchain.get_longest_chain_length();
                                    if propose.content.length > length {
                                        let vote = Message::Vote(Vote { content: propose.content, sender: self.environment.my_node.id });
                                        self.broadcast(connections, vote).await;
                                    }
                                }
                            },
                            Message::Vote(vote) => {
                                let nodes_voted = self.votes_ids.entry(vote.content.epoch).or_insert_with(Vec::new);
                                if !nodes_voted.contains(&vote.sender) {
                                    nodes_voted.push(vote.sender);
                                    if nodes_voted.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                                        let finalization = self.blockchain.add_block(&vote.content);
                                        if finalization {
                                            self.blocks.retain(|epoch, _| epoch >= &vote.content.epoch);
                                            self.votes_ids.retain(|epoch, _| epoch >= &vote.content.epoch)
                                        }
                                    }
                                    let message = Message::Vote(vote);
                                    self.broadcast(connections, message).await;
                                }
                            },
                        }
                    },
                    Err(_) => continue
                }
            }
        }
    }

    fn get_leader(&self, epoch: u32, rng: &mut StdRng) -> u32 {
        if epoch < CONFUSION_START || epoch >= CONFUSION_START + CONFUSION_DURATION {
            rng.random::<u32>() % (self.environment.nodes.len() as u32)
        } else {
            (epoch - 1) % (self.environment.nodes.len() as u32)
        }
    }

    async fn propose(&mut self, connections: &mut Vec<TcpStream>) {
        let epoch = self.epoch.load(Ordering::SeqCst);
        let transactions = self.transaction_generator.generate(self.environment.my_node.id);
        let block = self.blockchain.get_next_block(epoch, transactions);
        let message = Message::Propose(Propose { content: block, sender: self.environment.my_node.id });
        self.broadcast(connections, message).await;
    }

    async fn broadcast(&self, connections: &mut Vec<TcpStream>, message: Message) {
        let serialized_message = match to_string(&message) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("Failed to serialize message: {}", e);
                return;
            }
        };

        let serialized_bytes = serialized_message.as_bytes();
        let length = serialized_bytes.len() as u32;
        let length_bytes = length.to_be_bytes();
        let signature = self.private_key.sign(serialized_bytes);

        for i in (0..connections.len()).rev() {
            let stream = &mut connections[i];
            if let Err(e) = stream.write_all(&length_bytes).await {
                eprintln!("Failed to send message length to socket: {}", e);
                connections.remove(i);
                continue;
            }
            if let Err(e) = stream.write_all(serialized_bytes).await {
                eprintln!("Failed to send message to socket: {}", e);
                connections.remove(i);
                continue;
            }
            if let Err(e) = stream.write_all(signature.as_ref()).await {
                eprintln!("Failed to send signature to socket: {}", e);
                connections.remove(i);
                continue;
            }
        }
    }
}


async fn handle_connection(mut socket: TcpStream, sender: Arc<Sender<Message>>, public_key: &Vec<u8>) {
    let public_key = UnparsedPublicKey::new(&ED25519, public_key);
    loop {
        let mut length_bytes = [0; MESSAGE_BYTES_LENGTH];
        match socket.read_exact(&mut length_bytes).await {
            Ok(_) => {}
            Err(e) => {
                println!("Error reading length bytes: {}", e);
                return;
            }
        };
        let length = u32::from_be_bytes(length_bytes);
        let mut buffer = vec![0; length as usize];
        match socket.read_exact(&mut buffer).await {
            Ok(_) => {}
            Err(e) => {
                println!("Error reading message bytes: {}", e);
                return;
            }
        };
        let mut signature = vec![0; MESSAGE_SIGNATURE_BYTES_LENGTH];
        match socket.read_exact(&mut signature).await {
            Ok(_) => {}
            Err(e) => {
                println!("Error reading length bytes: {}", e);
                return;
            }
        };
        match public_key.verify(&buffer, &signature) {
            Ok(_) => {
                match from_slice::<Message>(&buffer) {
                    Ok(message) => {
                        match sender.send(message).await {
                            Ok(_) => {},
                            Err(_) => return
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize message: {}", e);
                        return;
                    }
                }
            },
            Err(_) => {
                println!("Signature verification failed.");
                return;
            }
        }
    }
}
