use crate::message::{BaseMessage, Echo, Propose, StreamletMessage, Vote};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use bincode::{deserialize, serialize};
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer, Verifier};
use log::{error, info, warn};
use serde_json::to_string;
use sha2::{Digest, Sha256};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time;
use tokio::time::sleep;
use shared::initializer::get_private_key;
use crate::block::{Epoch, NodeId, StreamletBlock};
use crate::blockchain::Blockchain;
use crate::connection::{broadcast, generate_nonce};

pub struct Latency {
    pub start: SystemTime,
    pub finalization: Option<SystemTime>,
}

const INITIAL_EPOCH: u32 = 0;
const EPOCH_TIME: f64 = 0.04;
const MESSAGE_CHANNEL_SIZE: usize = 100;
const NONCE_BYTES_LENGTH: usize = 32;
const MESSAGE_BYTES_LENGTH: usize = 4;
const SIGNATURE_BYTES_LENGTH: usize = 64;
const EXECUTION_TIME_SECS: u64 = 60;
const RESET_TIMER_CHANNEL_SIZE: usize = 100;
const SOCKET_BINDING_DELAY: u64 = 5;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks_";

pub struct Streamlet {
    environment: Environment,
    epoch: Arc<AtomicU32>,
    quorum_size: usize,
    transaction_generator: TransactionGenerator,
    blocks: HashMap<u32, bool>,
    votes_ids: HashMap<StreamletBlock, Vec<NodeId>>,
    blockchain: Blockchain,
    n_finalized_blocks: usize,
    finalization_timestamps: HashMap<Epoch, Latency>,
    public_keys: HashMap<NodeId, PublicKey>,
    private_key: Keypair,
}

impl Streamlet {

    pub fn new(environment: Environment, public_keys: HashMap<NodeId, PublicKey>, private_key: Keypair) -> Self {
        let n = environment.nodes.len();
        let transaction_size = environment.transaction_size;
        let n_transactions = environment.n_transactions;
        Streamlet {
            environment,
            epoch: Arc::new(AtomicU32::new(INITIAL_EPOCH)),
            quorum_size: n * 2 / 3 + 1,
            transaction_generator: TransactionGenerator::new(transaction_size, n_transactions),
            blocks: HashMap::new(),
            votes_ids: HashMap::new(),
            blockchain: Blockchain::new(),
            n_finalized_blocks: 0,
            finalization_timestamps: HashMap::new(),
            public_keys,
            private_key
        }
    }

    pub async fn start(mut self) {
        let address = format!("{}:{}", self.environment.my_node.host, self.environment.my_node.port);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let (message_queue_sender, message_queue_receiver) = mpsc::channel::<(NodeId, StreamletMessage)>(MESSAGE_CHANNEL_SIZE);
                let (dispatcher_queue_sender, dispatcher_queue_receiver) = mpsc::channel::<StreamletMessage>(MESSAGE_CHANNEL_SIZE);
                let (finalize_sender, finalize_receiver) = mpsc::channel::<Vec<StreamletBlock>>(MESSAGE_CHANNEL_SIZE);
                let (epoch_sender, epoch_receiver) = mpsc::channel::<()>(RESET_TIMER_CHANNEL_SIZE);
                sleep(Duration::from_secs(SOCKET_BINDING_DELAY)).await;
                let connections = self.connect(&message_queue_sender, &dispatcher_queue_sender, listener).await;
                self.start_message_dispatcher(&message_queue_sender, dispatcher_queue_receiver, connections).await;
                self.start_epoch_counter(epoch_sender).await;
                self.start_finalize_task(finalize_receiver).await;
                self.execute_protocol(message_queue_receiver, dispatcher_queue_sender, epoch_receiver, finalize_sender).await;
            }
            Err(_) => { error!("[Node {}] Failed to bind local port", self.environment.my_node.id) }
        };
    }

    async fn connect(
        &self,
        message_queue_sender: &Sender<(NodeId, StreamletMessage)>,
        dispatcher_queue_sender: &Sender<StreamletMessage>,
        listener: TcpListener
    ) -> Vec<Option<TcpStream>> {
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
                    let dispatcher_queue_sender = dispatcher_queue_sender.clone();
                    let message_queue_sender = message_queue_sender.clone();
                    let enable_crypto = !self.environment.test_flag;
                    let my_node_id = self.environment.my_node.id;
                    let public_keys = self.public_keys.clone();
                    tokio::spawn(async move {
                        Self::handle_connection(enable_crypto, stream, dispatcher_queue_sender, message_queue_sender, &public_keys, my_node_id, claimed_id).await;
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
            StreamletMessage::Base(_) => {
                broadcast(private_key, connections, &message, enable_crypto).await;
                let _ = message_queue_sender.send((my_node_id, message)).await;
            }
            StreamletMessage::Echo(_) => {
                broadcast(private_key, connections, &message, enable_crypto).await;
            }
        }
    }

    async fn handle_connection(
        enable_crypto: bool,
        mut stream: TcpStream,
        dispatcher_queue_sender: Sender<StreamletMessage>,
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

            if !message.is_echo() {
                if enable_crypto {
                    let mut signature = vec![0; SIGNATURE_BYTES_LENGTH];
                    if stream.read_exact(&mut signature).await.is_err() {
                        error!("[Node {}] Error reading message signature from Node {}", my_node_id, node_id);
                        return;
                    }
                    let sig = Signature::from_bytes(&signature).unwrap();
                    if !public_key.verify(&buffer, &sig).is_ok() {
                        continue;
                    }
                    let _ = dispatcher_queue_sender.send(StreamletMessage::Echo(Echo { message: message.get_base_message().unwrap(), signature })).await;
                } else {
                    let _ = dispatcher_queue_sender.send(StreamletMessage::Echo(Echo { message: message.get_base_message().unwrap(), signature: vec![] })).await;
                }
                let _ = message_queue_sender.send((node_id, message)).await;
            } else {
                if enable_crypto {
                    if let (Some(echo), Some(signature) ) = (message.get_echo_message(), message.get_echo_signature()) {
                        let sig = Signature::from_bytes(signature).unwrap();
                        let payload = match serialize(&echo) {
                            Ok(msg) => msg,
                            Err(e) => {
                                error!("Failed to serialize message: {}", e);
                                return;
                            }
                        };
                        let entry = public_keys.iter().find(|(_, pk)| pk.verify(&payload, &sig).is_ok());
                        match entry {
                            None => { continue }
                            Some((original_sender, _)) => {
                                let _ = message_queue_sender.send((*original_sender, echo)).await;
                            }
                        }
                    }
                }
            }
        }
    }

    async fn start_epoch_counter(&self, epoch_sender: Sender<()>) {
        let epoch_counter = self.epoch.clone();
        let mut interval = time::interval(Duration::from_secs_f64(EPOCH_TIME));
        tokio::spawn(async move {
            loop {
                interval.tick().await;
                epoch_counter.fetch_add(1, Ordering::SeqCst);
                let _ = epoch_sender.send(()).await;
                interval = time::interval(Duration::from_secs_f64(EPOCH_TIME));
                interval.tick().await;
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
        let start = time::Instant::now();
        while start.elapsed() < Duration::from_secs(EXECUTION_TIME_SECS) {
            tokio::select! {
                _ = epoch_receiver.recv() => {
                    let epoch = self.epoch.load(Ordering::SeqCst);
                    self.finalization_timestamps.insert(epoch, Latency { start: SystemTime::now(), finalization: None });
                    let leader = self.get_leader(epoch);
                    info!("----------------------");
                    info!("----------------------");
                    info!("Leader is node {} | Epoch: {}", leader, epoch);
                    if leader == self.environment.my_node.id {
                        let epoch = self.epoch.load(Ordering::SeqCst);
                        let transactions = self.transaction_generator.generate();
                        let block = self.blockchain.get_next_block(epoch, transactions);
                        let message = StreamletMessage::Base(BaseMessage::Propose(Propose { content: block }));
                        let _ = dispatcher_queue_sender.send(message).await;
                    }
                }

                Some((sender, msg)) = message_queue_receiver.recv() => {
                    match msg {
                        StreamletMessage::Base(base_msg) => match base_msg {
                            BaseMessage::Propose(propose) => {
                                info!("Received propose for epoch {}", propose.content.epoch);
                                if !self.blocks.contains_key(&propose.content.epoch) {
                                    self.blocks.insert(propose.content.epoch, true);
                                    let length = self.blockchain.get_longest_chain().length;
                                    if propose.content.length == length + 1 {
                                        let vote = StreamletMessage::Base(BaseMessage::Vote(Vote { content: propose.content }));
                                        let _ = dispatcher_queue_sender.send(vote).await;
                                    }
                                }
                            }
                            BaseMessage::Vote(vote) => {
                                info!("Received vote for epoch {}", vote.content.epoch);
                                let nodes_voted = self.votes_ids.entry(vote.content.clone()).or_insert_with(Vec::new);
                                if !nodes_voted.contains(&sender) {
                                    nodes_voted.push(sender);
                                    if nodes_voted.len() == self.quorum_size {
                                        let finalized_epochs = self.blockchain.handle_notarization(&vote.content, &finalize_sender).await;
                                        if !finalized_epochs.is_empty()  {
                                            self.n_finalized_blocks += finalized_epochs.len();
                                            for epoch in finalized_epochs {
                                                if let Some(latency) = self.finalization_timestamps.get_mut(&epoch) {
                                                    latency.finalization = Some(SystemTime::now());
                                                }
                                            }
                                            self.blocks.retain(|epoch, _| *epoch >= vote.content.epoch);
                                            self.votes_ids.retain(|block, _| block.epoch >= vote.content.epoch)
                                        }
                                    }
                                }
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
        println!("Blocks finalized: {} ", self.n_finalized_blocks);
        println!("Average finalization time (ms): {}", self.get_finalization_time());
        std::process::exit(0);
    }

    fn get_leader(&self, epoch: Epoch) -> NodeId {
        let mut hasher = Sha256::new();
        hasher.update(&epoch.to_le_bytes());
        let hash = hasher.finalize();
        let hash_u64 = u64::from_le_bytes(hash[0..8].try_into().expect("Invalid slice length"));
        (hash_u64 % self.environment.nodes.len() as u64) as u32
    }

    fn get_finalization_time(&self) -> f64 {
        let mut total = Duration::from_millis(0);
        let mut count = 0.0;

        for latency in self.finalization_timestamps.values() {
            if let Some(finalization_time) = latency.finalization {
                if let Ok(diff) = finalization_time.duration_since(latency.start) {
                    total += diff;
                    count += 1.0;
                }
            }
        }

        if count > 0.0 {
            total.as_secs_f64() / count
        } else {
            0.0
        }
    }
}
