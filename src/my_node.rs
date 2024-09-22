use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, mpsc};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

use rand::{Rng, SeedableRng};
use rand::prelude::StdRng;
use serde_json::{from_slice, to_string};

use crate::blockchain::Blockchain;
use crate::domain::environment::Environment;
use crate::domain::message::{Message, Propose, Vote};
use crate::transaction_generator::TransactionGenerator;

const SEED: u64 = 0xFF6DA736EA;
const INITIAL_EPOCH: u32 = 0;
const EPOCH_TIME: u64 = 1;
const CONFUSION_START: u32 = 0;
const CONFUSION_DURATION: u32 = 0;
const MESSAGE_LENGTH_BYTES: usize = 4;


pub struct MyNode {
    environment: Environment,
    listener: TcpListener,
    server_sockets: Vec<TcpStream>,
    node_sockets: Vec<TcpStream>,
    epoch: Arc<AtomicU32>,
    is_new_epoch: Arc<AtomicBool>,
    blockchain: Blockchain,
    transaction_generator: TransactionGenerator,
    blocks: HashMap<u32, bool>, // epoch: received
    votes_ids: HashMap<u32, Vec<u32>>, // epoch: nodes that voted already
}

impl MyNode {

    pub fn new(environment: Environment) -> Self {
        let address = format!("{}:{}", environment.my_node.host, environment.my_node.port);
        let listener = TcpListener::bind(address).expect("Failed to bind to address"); //TODO()
        let my_node_id = environment.my_node.id;

        MyNode {
            environment,
            listener,
            server_sockets: Vec::new(),
            node_sockets: Vec::new(),
            epoch: Arc::new(AtomicU32::new(INITIAL_EPOCH)),
            is_new_epoch: Arc::new(AtomicBool::new(false)),
            blockchain: Blockchain::new(my_node_id),
            transaction_generator: TransactionGenerator::new(),
            blocks: HashMap::new(),
            votes_ids: HashMap::new(),
        }
    }

    fn connect(&mut self) {
        for node in self.environment.nodes.iter() {
            let address = format!("{}:{}", node.host, node.port);
            match TcpStream::connect(address) {
                Ok(stream) => self.node_sockets.push(stream),
                Err(e) => eprintln!("[Node {}] Failed to connect to node {}: {}", self.environment.my_node.id, node.id, e),
            }
        }
    }

    fn accept_connections(&mut self) {
        let mut accepted = 0;
        let nodes_count = self.environment.nodes.len();
        while accepted < nodes_count {
            match self.listener.accept() {
                Ok((stream, _addr)) => self.server_sockets.push(stream),
                Err(e) => eprintln!("[Node {}] Failed to accept a connection: {}", self.environment.my_node.id, e)
            }
            accepted += 1;
        }
    }

    pub fn start_streamlet(mut self) {
        self.connect();
        self.accept_connections();
        let (sender, receiver): (Sender<Message>, Receiver<Message>) = mpsc::channel();
        self.start_epoch_counter();
        self.listen_for_messages(sender);
        self.execute_protocol(receiver);
    }

    fn start_epoch_counter(&self) {
        let epoch_counter = self.epoch.clone();
        let is_new_epoch = self.is_new_epoch.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(EPOCH_TIME));
                epoch_counter.fetch_add(1, Ordering::SeqCst);
                loop {
                    match is_new_epoch.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
                        Ok(_) => break,
                        Err(_) => { }
                    }
                }
            }
        });
    }

    fn listen_for_messages(&self, message_queue_sender: Sender<Message>) {
        let sender = Arc::new(message_queue_sender);
        for node_socket in self.node_sockets.iter() {
            let mut socket = node_socket.try_clone().expect("Failed to clone socket");
            let sender = Arc::clone(&sender);
            thread::spawn(move || {
                loop {
                    let mut length_bytes = [0; MESSAGE_LENGTH_BYTES];
                    match socket.read_exact(&mut length_bytes) {
                        Ok(_) => {}
                        Err(e) => {
                            println!("Error reading length bytes: {}", e);
                            continue
                        }
                    };
                    let length = u32::from_be_bytes(length_bytes);
                    let mut buffer = vec![0; length as usize];
                    match socket.read_exact(&mut buffer) {
                        Ok(_) => {}
                        Err(_) => {
                            println!("Error reading message bytes");
                            continue
                        }
                    };
                    match from_slice::<Message>(&buffer) {
                        Ok(message) => {
                            match sender.send(message) {
                                Ok(..) => continue,
                                Err(..) => return
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize message: {}", e);
                        }
                    }
                }
            });
        }
    }

    fn execute_protocol(&mut self, message_queue_receiver: Receiver<Message>) {
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
                        self.propose();
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
                                    self.send_message_to_all_nodes(message);
                                    let length = self.blockchain.get_longest_chain_length();
                                    if propose.content.length > length {
                                        let vote = Message::Vote(Vote { content: propose.content, sender: self.environment.my_node.id });
                                        self.send_message_to_all_nodes(vote);
                                    }
                                }
                            },
                            Message::Vote(vote) => {
                                let nodes_voted = self.votes_ids.entry(vote.content.epoch).or_insert_with(Vec::new);
                                if !nodes_voted.contains(&vote.sender) {
                                    nodes_voted.push(vote.sender);
                                    if nodes_voted.len() == ((self.environment.nodes.len() as u32) / 2 + 1) as usize {
                                        self.blockchain.add_block(&vote.content);
                                    }
                                    let message = Message::Vote(vote);
                                    self.send_message_to_all_nodes(message);
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

    fn propose(&mut self) {
        let epoch = self.epoch.load(Ordering::SeqCst);
        let transactions = self.transaction_generator.generate(self.environment.my_node.id);
        let block = self.blockchain.get_next_block(epoch, transactions);
        let message = Message::Propose(Propose { content: block, sender: self.environment.my_node.id });
        self.send_message_to_all_nodes(message)
    }

    fn send_message_to_all_nodes(&self, message: Message) {
        let serialized_message = match to_string(&message) {
            Ok(json) => {
                //println!("Serialized: {}", json);
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
        for server_socket in self.server_sockets.iter() {
            let mut socket = server_socket.try_clone().expect("Failed to clone socket"); //TODO()
            if let Err(e) = socket.write_all(&length_bytes) {
                eprintln!("Failed to send message to socket: {}", e);
                continue;
            }
            if let Err(e) = socket.write_all(serialized_bytes) {
                eprintln!("Failed to send message to socket: {}", e);
            }
        }
    }
}
