use crate::blockchain::Blockchain;
use crate::message::{Propose, StreamletMessage, Vote};
use rand::prelude::StdRng;
use rand::{Rng, SeedableRng};
use ring::signature::Ed25519KeyPair;
use shared::connection::{accept_connections, broadcast, connect};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::time;

const SEED: u64 = 0xFF6DA736EA;
const INITIAL_EPOCH: u32 = 0;
const EPOCH_TIME: u64 = 3;
const CONFUSION_START: u32 = 0;
const CONFUSION_DURATION: u32 = 0;
const MESSAGE_CHANNEL_SIZE: usize = 50;


pub struct StreamletNode {
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

impl StreamletNode {

    pub fn new(environment: Environment, public_keys: HashMap<u32, Vec<u8>>, private_key: Ed25519KeyPair) -> Self {
        let my_node_id = environment.my_node.id;
        StreamletNode {
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


    pub async fn start_streamlet(mut self) {
        let address = format!("{}:{}", self.environment.my_node.host, self.environment.my_node.port);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let (tx, rx) = mpsc::channel::<StreamletMessage>(MESSAGE_CHANNEL_SIZE);
                let sender = Arc::new(tx);
                connect(self.environment.my_node.id, &self.environment.nodes, &self.public_keys, sender).await;
                let mut connections = accept_connections(self.environment.my_node.id, &self.environment.nodes, listener).await;
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

    async fn execute_protocol(&mut self, mut message_queue_receiver: Receiver<StreamletMessage>, connections: &mut Vec<TcpStream>) {
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
                            StreamletMessage::Propose(propose) => {
                                if !self.blocks.contains_key(&propose.content.epoch) {
                                    self.blocks.insert(propose.content.epoch, true);
                                    let message = StreamletMessage::Propose(Propose { content: propose.content.clone(), sender: propose.sender });
                                    broadcast(&self.private_key, connections, message).await;
                                    let length = self.blockchain.get_longest_chain_length();
                                    if propose.content.length > length {
                                        let vote = StreamletMessage::Vote(Vote { content: propose.content, sender: self.environment.my_node.id });
                                        broadcast(&self.private_key, connections, vote).await;
                                    }
                                }
                            },
                            StreamletMessage::Vote(vote) => {
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
                                    let message = StreamletMessage::Vote(vote);
                                    broadcast(&self.private_key, connections, message).await;
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
        let message = StreamletMessage::Propose(Propose { content: block, sender: self.environment.my_node.id });
        broadcast(&self.private_key, connections, message).await;
    }
}
