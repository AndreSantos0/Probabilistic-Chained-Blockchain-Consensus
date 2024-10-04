use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use rand::{Rng, SeedableRng};
use rand::prelude::StdRng;
use serde_json::{from_slice, to_string};
use tokio::{io, time};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::blockchain::Blockchain;
use crate::domain::environment::Environment;
use crate::domain::message::{Message, Propose, Vote};
use crate::transaction_generator::TransactionGenerator;

const SEED: u64 = 0xFF6DA736EA;
const INITIAL_EPOCH: u32 = 0;
const EPOCH_TIME: u64 = 3;
const CONFUSION_START: u32 = 0;
const CONFUSION_DURATION: u32 = 0;
const MESSAGE_LENGTH_BYTES: usize = 4;


pub struct MyNode {
    environment: Environment,
    epoch: Arc<AtomicU32>,
    is_new_epoch: Arc<AtomicBool>,
    blockchain: Blockchain,
    transaction_generator: TransactionGenerator,
    blocks: HashMap<u32, bool>, // epoch: received
    votes_ids: HashMap<u32, Vec<u32>>, // epoch: nodes that voted already
}

impl MyNode {

    pub fn new(environment: Environment) -> Self {
        let my_node_id = environment.my_node.id;
        MyNode {
            environment,
            epoch: Arc::new(AtomicU32::new(INITIAL_EPOCH)),
            is_new_epoch: Arc::new(AtomicBool::new(false)),
            blockchain: Blockchain::new(my_node_id),
            transaction_generator: TransactionGenerator::new(),
            blocks: HashMap::new(),
            votes_ids: HashMap::new(),
        }
    }

    async fn connect(&mut self) -> Vec<TcpStream> {
        let mut connections = Vec::new();
        for node in self.environment.nodes.iter() {
            let address = format!("{}:{}", node.host, node.port);
            match TcpStream::connect(address).await {
                Ok(stream) => connections.push(stream),
                Err(e) => eprintln!("[Node {}] Failed to connect to Node {}: {}", self.environment.my_node.id, node.id, e),
            }
        }
        connections
    }

    async fn accept_connections(&mut self, listener: TcpListener, sender: Arc<Sender<Message>>) {
        let mut accepted = 0;
        let n_nodes = self.environment.nodes.len();
        while accepted < n_nodes {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let sender = Arc::clone(&sender);
                    tokio::spawn(async {
                        handle_connection(stream, sender).await
                    });
                },
                Err(e) => eprintln!("[Node {}] Failed to accept a connection: {}", self.environment.my_node.id, e)
            }
            accepted += 1;
        }
    }

    pub async fn start_streamlet(mut self) -> io::Result<()> {
        let address = format!("{}:{}", self.environment.my_node.host, self.environment.my_node.port);
        let listener = TcpListener::bind(address).await?;
        let (tx, rx) = mpsc::channel::<Message>(50);
        let sender = Arc::new(tx);

        let mut connections = self.connect().await;
        self.accept_connections(listener, sender).await;
        self.start_epoch_counter();
        self.execute_protocol(rx, &mut connections).await;
        Ok(())
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
            let epoch = self.epoch.load(Ordering::SeqCst);
            match self.is_new_epoch.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => {
                    let leader = self.get_leader(&mut rng);
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

            if epoch < CONFUSION_START || epoch >= CONFUSION_START + CONFUSION_DURATION {
                match message_queue_receiver.try_recv() {
                    Ok(msg) => {
                        match msg {
                            Message::Propose(propose) => {
                                if !self.blocks.contains_key(&propose.content.epoch) {
                                    self.blocks.insert(propose.content.epoch, true);
                                    let message = Message::Propose(Propose { content: propose.content.clone(), sender: self.environment.my_node.id });
                                    send_message_to_all_nodes(connections, message).await;
                                    let length = self.blockchain.get_longest_chain_length();
                                    if propose.content.length > length {
                                        let vote = Message::Vote(Vote { content: propose.content, sender: self.environment.my_node.id });
                                        send_message_to_all_nodes(connections, vote).await;
                                    }
                                }
                            },
                            Message::Vote(vote) => {
                                let nodes_voted = self.votes_ids.entry(vote.content.epoch).or_insert_with(Vec::new);
                                if !nodes_voted.contains(&vote.sender) {
                                    nodes_voted.push(vote.sender);
                                    if nodes_voted.len() == ((self.environment.nodes.len() as u32) / 2 + 1) as usize {
                                        let finalization = self.blockchain.add_block(&vote.content);
                                        if finalization {
                                            self.blocks.retain(|epoch, _| epoch >= &vote.content.epoch);
                                            self.votes_ids.retain(|epoch, _| epoch >= &vote.content.epoch)
                                        }
                                    }
                                    let message = Message::Vote(vote);
                                    send_message_to_all_nodes(connections, message).await;
                                }
                            },
                        }
                    },
                    Err(_) => continue
                }
            }
        }
    }

    fn get_leader(&self, rng: &mut StdRng) -> u32 {
        let epoch = self.epoch.load(Ordering::SeqCst);
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
        send_message_to_all_nodes(connections, message).await;
    }
}

async fn send_message_to_all_nodes(connections: &mut Vec<TcpStream>, message: Message) {
    let serialized_message = match to_string(&message) {
        Ok(json) => {
            json
        },
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    let serialized_bytes = serialized_message.as_bytes();
    let length = serialized_bytes.len() as u32;
    let length_bytes = length.to_be_bytes();
    for stream in connections.iter_mut() {
        if let Err(e) = stream.write_all(&length_bytes).await {
            eprintln!("Failed to send message to socket: {}", e);
            continue;
        }
        if let Err(e) = stream.write_all(serialized_bytes).await {
            eprintln!("Failed to send message to socket: {}", e);
        }
    }
}

async fn handle_connection(mut socket: TcpStream, sender: Arc<Sender<Message>>) {
    loop {
        let mut length_bytes = [0; MESSAGE_LENGTH_BYTES];
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
        match from_slice::<Message>(&buffer) {
            Ok(message) => {
                match sender.send(message).await {
                    Ok(..) => continue,
                    Err(..) => return
                }
            }
            Err(e) => {
                eprintln!("Failed to deserialize message: {}", e);
                return;
            }
        }
    }
}