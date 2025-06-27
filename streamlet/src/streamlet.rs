use crate::message::{Propose, StreamletMessage, Vote};
use rand::prelude::StdRng;
use rand::{Rng, SeedableRng};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use bincode::{deserialize, serialize};
use ed25519_dalek::{verify_batch, Keypair, PublicKey, Signature, Signer, Verifier};
use log::{error, warn};
use serde_json::to_string;
use sha2::Sha256;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time;
use tokio::time::sleep;
use shared::initializer::get_private_key;
use crate::block::{NodeId, StreamletBlock};
use crate::connection::{broadcast, generate_nonce};

const SEED: u64 = 0xFF6DA736EA;
const INITIAL_EPOCH: u32 = 0;
const EPOCH_TIME: u64 = 3;
const CONFUSION_START: u32 = 0;
const CONFUSION_DURATION: u32 = 0;
const MESSAGE_CHANNEL_SIZE: usize = 100;
const NONCE_BYTES_LENGTH: usize = 32;
const MESSAGE_BYTES_LENGTH: usize = 4;
const SIGNATURE_BYTES_LENGTH: usize = 64;
const EXECUTION_TIME_SECS: u64 = 60;
const RESET_TIMER_CHANNEL_SIZE: usize = 100;
const SOCKET_BINDING_DELAY: u64 = 5;
const GENESIS_ITERATION: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const INITIAL_FINALIZED_HEIGHT: u32 = 0;
const INITIAL_ITERATION: u32 = 1;
const ITERATION_TIME: u64 = 5;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks_";

pub struct Streamlet {
    environment: Environment,
    epoch: Arc<AtomicU32>,
    transaction_generator: TransactionGenerator,
    blocks: HashMap<u32, bool>,
    votes_ids: HashMap<u32, Vec<u32>>,
    public_keys: HashMap<NodeId, PublicKey>,
    private_key: Keypair,
}

impl Streamlet {

    pub fn new(environment: Environment, public_keys: HashMap<NodeId, PublicKey>, private_key: Keypair) -> Self {
        let transaction_size = environment.transaction_size;
        let n_transactions = environment.n_transactions;
        Streamlet {
            environment,
            epoch: Arc::new(AtomicU32::new(INITIAL_EPOCH)),
            transaction_generator: TransactionGenerator::new(transaction_size, n_transactions),
            blocks: HashMap::new(),
            votes_ids: HashMap::new(),
            public_keys,
            private_key
        }
    }

    pub async fn start(mut self) {
        let address = format!("{}:{}", self.environment.my_node.host, self.environment.my_node.port);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let (consumer_queue_sender, consumer_queue_receiver) = mpsc::channel::<(NodeId, StreamletMessage)>(MESSAGE_CHANNEL_SIZE);
                let (dispatcher_queue_sender, dispatcher_queue_receiver) = mpsc::channel::<StreamletMessage>(MESSAGE_CHANNEL_SIZE);
                let (finalize_sender, finalize_receiver) = mpsc::channel::<Vec<StreamletBlock>>(MESSAGE_CHANNEL_SIZE);
                let (epoch_sender, epoch_receiver) = mpsc::channel::<()>(RESET_TIMER_CHANNEL_SIZE);
                sleep(Duration::from_secs(SOCKET_BINDING_DELAY)).await;
                let connections = self.connect(consumer_queue_sender.clone(), listener).await;
                self.start_message_dispatcher(&consumer_queue_sender, dispatcher_queue_receiver, connections).await;
                self.start_epoch_counter(epoch_sender);
                self.start_finalize_task(finalize_receiver).await;
                self.execute_protocol(consumer_queue_receiver, dispatcher_queue_sender, epoch_receiver, finalize_sender).await;
            }
            Err(_) => { error!("[Node {}] Failed to bind local port", self.environment.my_node.id) }
        };
    }

    async fn connect(&self, message_queue_sender: Sender<(NodeId, StreamletMessage)>, listener: TcpListener) -> Vec<Option<TcpStream>> {
        let mut connections = Vec::new();
        for node in self.environment.nodes.iter() {
            if node.id != self.environment.my_node.id {
                let address = format!("{}:{}", node.host, node.port);
                let mut stream = TcpStream::connect(address).await.expect(&format!("[Node {}] Failed to connect to Node {}", self.environment.my_node.id, node.id));
                let node_id_bytes = self.environment.my_node.id.to_be_bytes();
                let nonce = generate_nonce();
                let signature = self.private_key.sign(&nonce);
                stream.write_all(&node_id_bytes).await.expect(&format!("[Node {}] Failed writing during handshake to Node {}", self.environment.my_node.id, node.id));
                stream.write_all(&nonce).await.expect(&format!("[Node {}] Failed writing during handshake to Node {}", self.environment.my_node.id, node.id));
                stream.write_all((&signature).as_ref()).await.expect(&format!("[Node {}] Failed writing during handshake to Node {}", self.environment.my_node.id, node.id));
                connections.push(Some(stream));
            }
        }

        let mut accepted = 0;
        while accepted != self.environment.nodes.len() - 1 {
            let (mut stream, _) = listener.accept().await.expect(&format!("[Node {}] Failed accepting incoming connection", self.environment.my_node.id));
            let mut id_buf = [0u8; 4];
            stream.read_exact(&mut id_buf).await.expect(&format!("[Node {}] Failed reading during handshake", self.environment.my_node.id));
            let claimed_id = u32::from_be_bytes(id_buf);
            let mut nonce = vec![0u8; NONCE_BYTES_LENGTH];
            stream.read_exact(&mut nonce).await.expect(&format!("[Node {}] Failed reading during handshake", self.environment.my_node.id));
            let mut sig = vec![0u8; SIGNATURE_BYTES_LENGTH];
            stream.read_exact(&mut sig).await.expect(&format!("[Node {}] Failed reading during handshake", self.environment.my_node.id));
            if let Some(key) = self.public_keys.get(&claimed_id) {
                if key.verify(&nonce, &Signature::from_bytes(&sig).unwrap()).is_ok() {
                    let sender = message_queue_sender.clone();
                    let enable_crypto = !self.environment.test_flag;
                    let my_node_id = self.environment.my_node.id;
                    let public_keys = self.public_keys.clone();
                    tokio::spawn(async move {
                        Self::handle_connection(enable_crypto, stream, sender, &public_keys, my_node_id, claimed_id).await;
                    });
                } else {
                    warn!("[Node {}] Invalid signature from claimed Node {}", self.environment.my_node.id, claimed_id);
                    stream.shutdown().await.expect(&format!("[Node {}] Failed to shutdown stream", self.environment.my_node.id));
                    continue
                }
            } else {
                warn!("[Node {}] Invalid connection from unknown node_id {}", self.environment.my_node.id, claimed_id);
                stream.shutdown().await.expect(&format!("[Node {}] Failed to shutdown stream", self.environment.my_node.id));
                continue
            }
            accepted += 1;
        }
        connections
    }

    async fn start_message_dispatcher(
        &self,
        message_queue_sender: &Sender<(NodeId, StreamletMessage)>,
        mut dispatcher_queue_receiver: Receiver<StreamletMessage>,
        mut connections: Vec<Option<TcpStream>>
    ) {
        let my_node_id = self.environment.my_node.id;
        let private_key = get_private_key(my_node_id);
        let enable_crypto = !self.environment.test_flag;
        let message_queue_sender = message_queue_sender.clone();
        tokio::spawn(async move {
            loop {
                if let Some(message) = dispatcher_queue_receiver.recv().await {
                    Self::dispatch_message(&message_queue_sender, message, &private_key, enable_crypto, my_node_id, &mut connections).await;
                }
            }
        });
    }

    async fn dispatch_message(
        message_queue_sender: &Sender<(NodeId, StreamletMessage)>,
        message: StreamletMessage,
        private_key: &Keypair,
        enable_crypto: bool,
        my_node_id: u32,
        connections: &mut Vec<Option<TcpStream>>
    ) {
        match message {
            StreamletMessage::Propose(_) => {
                broadcast(private_key, connections, &message, enable_crypto).await;
                let _ = message_queue_sender.send((my_node_id, message)).await;
            }
            StreamletMessage::Vote(_) => {
                broadcast(private_key, connections, &message, enable_crypto).await;
                let _ = message_queue_sender.send((my_node_id, message)).await;
            }
        }
    }

    async fn handle_connection(
        enable_crypto: bool,
        mut stream: TcpStream,
        message_queue_sender: Sender<(NodeId, StreamletMessage)>,
        public_keys: &HashMap<u32, PublicKey>,
        my_node_id: NodeId,
        node_id: NodeId,
    ) {
        let public_key = public_keys.get(&node_id).expect(&format!("[Node {}] Error getting public key of Node {}", my_node_id, node_id));
        loop {
            let mut length_bytes = [0; MESSAGE_BYTES_LENGTH];
            if stream.read_exact(&mut length_bytes).await.is_err() {
                error!("[Node {}] Error reading length bytes from Node {}", my_node_id, node_id);
                return;
            }

            let length = u32::from_be_bytes(length_bytes);
            let mut buffer = vec![0; length as usize];
            if stream.read_exact(&mut buffer).await.is_err() {
                error!("[Node {}] Error reading message bytes from Node {}", my_node_id, node_id);
                return;
            }

            let message: StreamletMessage = match deserialize(&buffer) {
                Ok(msg) => msg,
                Err(_) => {
                    error!("[Node {}] Error deserializing message from Node {}", my_node_id, node_id);
                    return;
                }
            };

            if enable_crypto {
                let mut signature = vec![0; SIGNATURE_BYTES_LENGTH];
                if stream.read_exact(&mut signature).await.is_err() {
                    error!("[Node {}] Error reading message signature from Node {}", my_node_id, node_id);
                    return;
                }
                if !public_key.verify(&buffer, &Signature::from_bytes(&signature).unwrap()).is_ok() {
                    continue;
                }
            }

            let _ = message_queue_sender.send((node_id, message)).await;
        }
    }

    fn start_epoch_counter(&self, epoch_sender: Sender<()>) {
        let epoch_counter = self.epoch.clone();
        //let epoch_sender = epoch_sender.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(EPOCH_TIME));
            loop {
                interval.tick().await;
                epoch_counter.fetch_add(1, Ordering::SeqCst);
                let _ = epoch_sender.send(());
                interval = time::interval(Duration::from_secs(EPOCH_TIME));
            }
        });
    }

    async fn start_finalize_task(&self, mut finalize_receiver: Receiver<Vec<StreamletBlock>>) {
        let my_node_id = self.environment.my_node.id;
        tokio::spawn(async move {
            loop {
                if let Some(blocks) = finalize_receiver.recv().await {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .append(true)
                        .open(format!("{}{}.ndjson", FINALIZED_BLOCKS_FILENAME, my_node_id))
                        .await
                        .expect("Could not open blockchain file");

                    let block_data = blocks.iter().map(|notarized| {
                        to_string(notarized).expect("Failed to serialize block")+ "\n"
                    })
                        .collect::<Vec<String>>()
                        .join("");
                    file.write_all(block_data.as_bytes()).await.expect("Error writing blocks to file");
                }
            }
        });
    }

    async fn execute_protocol(
        &mut self,
        mut message_queue_receiver: Receiver<(NodeId, StreamletMessage)>,
        dispatcher_queue_sender: Sender<StreamletMessage>,
        mut epoch_receiver: Receiver<()>,
        finalize_sender: Sender<Vec<StreamletBlock>>,
    ) {
        let mut rng = StdRng::seed_from_u64(SEED);
        loop {
            tokio::select! {
                _ = epoch_receiver.recv() => {
                    let epoch = self.epoch.load(Ordering::SeqCst);
                    let leader = self.get_leader(epoch, &mut rng);
                    println!("----------------------");
                    println!("----------------------");
                    println!("Leader is node {} | Epoch: {}", leader, epoch);
                    if leader == self.environment.my_node.id {
                        let epoch = self.epoch.load(Ordering::SeqCst);
                        let transactions = self.transaction_generator.generate();
                        //let block = self.blockchain.get_next_block(epoch, transactions);
                        //let message = StreamletMessage::Propose(Propose { content: block });
                        //let _ = dispatcher_queue_sender.send(message).await;
                    }
                }

                Some((sender, msg)) = message_queue_receiver.recv() => {
                    match msg {
                        StreamletMessage::Propose(propose) => {
                            if !self.blocks.contains_key(&propose.content.epoch) {
                                self.blocks.insert(propose.content.epoch, true);
                                let message = StreamletMessage::Propose(Propose { content: propose.content.clone() });
                                let _ = dispatcher_queue_sender.send(message).await;
                                //let length = self.blockchain.get_longest_chain_length();
                                //if propose.content.length > length {
                                //    let vote = StreamletMessage::Vote(Vote { content: propose.content });
                                //    let _ = dispatcher_queue_sender.send(vote).await;
                                //}
                            }
                        },
                        StreamletMessage::Vote(vote) => {
                            let nodes_voted = self.votes_ids.entry(vote.content.epoch).or_insert_with(Vec::new);
                            if !nodes_voted.contains(&sender) {
                                nodes_voted.push(sender);
                                if nodes_voted.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                                    //let finalization = self.blockchain.add_block(&vote.content);
                                    //if finalization {
                                    //    self.blocks.retain(|epoch, _| epoch >= &vote.content.epoch);
                                    //    self.votes_ids.retain(|epoch, _| epoch >= &vote.content.epoch)
                                    //}
                                }
                                let vote = StreamletMessage::Vote(vote);
                                let _ = dispatcher_queue_sender.send(vote).await;
                            }
                        },
                    }
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
}
