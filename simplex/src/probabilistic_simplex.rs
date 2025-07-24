use crate::block::{SimplexBlockHeader, SimplexBlock, VoteSignature, NodeId, NotarizedBlock, Iteration, hash};
use crate::connection::{broadcast, generate_nonce, unicast, MESSAGE_BYTES_LENGTH, NONCE_BYTES_LENGTH, SIGNATURE_BYTES_LENGTH};
use crate::message::{Dispatch, ProbFinalize, ProbPropose, ProbVote, ProbabilisticSimplexMessage, Reply, Request, SimplexMessage, Timeout};
use crate::protocol::{Latency, Protocol};
use sha2::{Digest, Sha256};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use shared::vrf::{vrf_prove, vrf_verify};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use bincode::{deserialize, serialize};
use ed25519_dalek::{verify_batch, Keypair, PublicKey, Signature, Signer, Verifier};
use log::{error, info, warn};
use rand::{rng, RngCore, SeedableRng};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use shared::domain::transaction::Transaction;
use shared::initializer::get_private_key;

pub struct ProbabilisticSimplex {
    environment: Environment,
    quorum_size: usize,
    probabilistic_quorum_size: usize,
    sample_size: usize,
    iteration: Arc<AtomicU32>,
    is_timeout: Arc<AtomicBool>,
    blocks_finalized: usize,
    proposes: HashMap<Iteration, SimplexBlockHeader>,
    transactions: HashMap<Iteration, Vec<Transaction>>,
    propose_certificates: HashMap<Iteration, Vec<VoteSignature>>,
    votes: HashMap<SimplexBlockHeader, Vec<VoteSignature>>,
    timeouts: HashMap<Iteration, Vec<NodeId>>,
    finalizes: HashMap<Iteration, Vec<NodeId>>,
    finalized_height: Iteration,
    to_be_finalized: Vec<Iteration>,
    finalization_timestamps: HashMap<Iteration, Latency>,
    transaction_generator: TransactionGenerator,
    public_keys: HashMap<NodeId, PublicKey>,
    private_key: Keypair,
}

const CONST_O: f32 = 1.7;
const CONST_L: f32 = 1.0;
const FINALIZATION_GAP: u32 = 10;

impl Protocol for ProbabilisticSimplex {

    type Message = ProbabilisticSimplexMessage;

    fn new(environment: Environment, public_keys: HashMap<u32, PublicKey>, private_key: Keypair) -> Self {
        let n = environment.nodes.len();
        let transaction_size = environment.transaction_size;
        let n_transactions = environment.n_transactions;
        let mut protocol = ProbabilisticSimplex {
            environment,
            quorum_size: n * 2 / 3 + 1,
            probabilistic_quorum_size: (CONST_L * (n as f32).sqrt()).floor() as usize,
            sample_size: (CONST_O * CONST_L * (n as f32).sqrt()).floor() as usize,
            iteration: Arc::new(AtomicU32::new(Self::INITIAL_ITERATION)),
            is_timeout: Arc::new(AtomicBool::new(false)),
            blocks_finalized: 0,
            proposes: HashMap::new(),
            transactions: HashMap::new(),
            propose_certificates: HashMap::new(),
            votes: HashMap::new(),
            timeouts: HashMap::new(),
            finalizes: HashMap::new(),
            finalized_height: Self::INITIAL_FINALIZED_HEIGHT,
            to_be_finalized: Vec::new(),
            finalization_timestamps: HashMap::new(),
            transaction_generator: TransactionGenerator::new(transaction_size, n_transactions),
            public_keys,
            private_key
        };
        protocol.add_genesis_block();
        protocol
    }

    fn get_environment(&self) -> &Environment {
        &self.environment
    }

    fn get_iteration(&self) -> &Arc<AtomicU32> {
        &self.iteration
    }

    fn get_is_timeout(&self) -> &Arc<AtomicBool> {
        &self.is_timeout
    }

    fn get_finalized_blocks(&self) -> usize {
        self.blocks_finalized
    }

    async fn connect(&self, message_queue_sender: Sender<(NodeId, ProbabilisticSimplexMessage)>, listener: TcpListener) -> Vec<Option<TcpStream>> {
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
                    let sample_size = self.sample_size;
                    let n_nodes = self.environment.nodes.len();
                    let probabilistic_quorum_size = self.probabilistic_quorum_size;
                    tokio::spawn(async move {
                        Self::handle_connection(enable_crypto, stream, sender, &public_keys, sample_size, n_nodes, probabilistic_quorum_size, my_node_id, claimed_id).await;
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
        message_queue_sender: &Sender<(NodeId, ProbabilisticSimplexMessage)>,
        mut dispatcher_queue_receiver: Receiver<Dispatch>,
        mut connections: Vec<Option<TcpStream>>
    ) {
        let my_node_id = self.environment.my_node.id;
        let private_key = get_private_key(my_node_id);
        let enable_crypto = !self.environment.test_flag;
        let message_queue_sender = message_queue_sender.clone();
        let sample_size = self.sample_size;
        let n_nodes = self.environment.nodes.len();
        tokio::spawn(async move {
            loop {
                if let Some(message) = dispatcher_queue_receiver.recv().await {
                    Self::dispatch_message(&message_queue_sender, message, &private_key, enable_crypto, sample_size, n_nodes, my_node_id, &mut connections).await;
                }
            }
        });
    }

    async fn handle_iteration_advance(
        &mut self,
        dispatcher_queue_sender: &Sender<Dispatch>,
        reset_timer_sender: &Sender<()>,
        finalize_sender: &Sender<Vec<NotarizedBlock>>
    ) {
        loop {
            let iteration = self.iteration.load(Ordering::Acquire);
            let leader = Self::get_leader(self.environment.nodes.len(), iteration);
            self.is_timeout.store(false, Ordering::Release);
            self.clear_timeouts(iteration);
            info!("----------------------");
            info!("----------------------");
            info!("Leader is node {} | Iteration: {}", leader, iteration);

            let my_node_id = self.environment.my_node.id;
            if leader == my_node_id {
                let transactions = self.transaction_generator.generate();
                let block = self.get_next_block(iteration, transactions);
                info!("Proposed {}", block.length);
                let propose = self.create_proposal(block);
                self.finalization_timestamps.insert(iteration, Latency { start: SystemTime::now(), finalization: None });
                let _ = dispatcher_queue_sender.send(propose).await;
            }

            if let Some(propose_header) = self.proposes.get(&iteration) {
                if self.propose_certificates.contains_key(&iteration) {
                    if propose_header.iteration > 1 {
                        let certificate_votes = self.propose_certificates.get(&iteration).unwrap();
                        let serialized_message = match serialize(&propose_header) {
                            Ok(msg) => msg,
                            Err(e) => {
                                error!("Failed to serialize message: {}", e);
                                return;
                            }
                        };

                        let messages: Vec<&[u8]> = (0..certificate_votes.len()).map(|_| serialized_message.as_slice()).collect();
                        let signatures: Vec<Signature> = certificate_votes.iter().map(
                            |vote_signature| Signature::from_bytes(vote_signature.signature.as_ref()).unwrap()
                        ).collect();
                        let mut keys = Vec::with_capacity(certificate_votes.len());
                        for vote_signature in certificate_votes {
                            match self.public_keys.get(&vote_signature.node) {
                                Some(key) => keys.push(*key),
                                None => return,
                            }
                        }

                        if !verify_batch(&messages[..], &signatures[..], &keys[..]).is_ok() {
                            return;
                        }
                        self.votes.insert(propose_header.clone(), self.propose_certificates.remove(&iteration).unwrap());
                    }

                    if self.is_extendable(propose_header) {
                        let vote = Self::create_vote(iteration, propose_header.clone());
                        let _ = dispatcher_queue_sender.send(vote).await;
                    };
                    if propose_header.iteration > 1 {
                        self.handle_notarization(iteration, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await;
                        self.finalize(iteration, finalize_sender).await;
                    }

                    continue
                }
            }

            break
        }
    }

    async fn handle_message(&mut self, sender: u32, message: Self::Message, dispatcher_queue_sender: &Sender<Dispatch>, reset_timer_sender: &Sender<()>, finalize_sender: &Sender<Vec<NotarizedBlock>>) {
        match message {
            ProbabilisticSimplexMessage::Propose(propose) => self.handle_propose(propose, sender, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await,
            ProbabilisticSimplexMessage::Vote(vote) => self.handle_vote(vote, sender, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await,
            _ => {}
        }
    }

    fn create_proposal(&self, block: SimplexBlock) -> Dispatch {
        let (header, signatures) = self.last_notarized();
        Dispatch::ProbPropose(block, header.iteration, signatures.clone())
    }

    fn create_timeout(next_iteration: u32) -> Dispatch {
        Dispatch::Timeout(next_iteration)
    }

    fn create_vote(iteration: u32, block: SimplexBlockHeader) -> Dispatch {
        Dispatch::Vote(iteration, block)
    }

    fn create_finalize(iteration: u32) -> Dispatch {
        Dispatch::Finalize(iteration)
    }

    fn get_proposal(&self, iteration: u32) -> Option<&SimplexBlockHeader> {
        self.proposes.get(&iteration)
    }

    fn get_timeouts(&self, iteration: u32) -> usize {
        match self.timeouts.get(&(iteration + 1)) {
            None => 0,
            Some(timeouts) => timeouts.len()
        }
    }

    fn clear_timeouts(&mut self, iteration: u32) {
        self.timeouts.retain(|iter, _| *iter > iteration);
    }

    fn get_finalization_timestamps(&self) -> &HashMap<Iteration, Latency> {
        &self.finalization_timestamps
    }
}

impl ProbabilisticSimplex {

    async fn dispatch_message(
        message_queue_sender: &Sender<(NodeId, ProbabilisticSimplexMessage)>,
        content: Dispatch,
        private_key: &Keypair,
        enable_crypto: bool,
        sample_size: usize,
        n_nodes: usize,
        my_node_id: u32,
        connections: &mut Vec<Option<TcpStream>>
    ) {
        match content {
            Dispatch::ProbPropose(block, last_notarized_iter, last_notarized_cert) => {
                let propose = ProbabilisticSimplexMessage::Propose(ProbPropose { content: block, last_notarized_iter, last_notarized_cert });
                broadcast(private_key, connections, &propose, enable_crypto).await;
                let _ = message_queue_sender.send((my_node_id, propose)).await;
            }
            Dispatch::Vote(iteration, header) => {
                let leader = Self::get_leader(n_nodes, iteration + 1);
                let serialized_message = serialize(&header).unwrap();
                let signature = private_key.sign(&serialized_message);
                let vote = ProbabilisticSimplexMessage::Vote(ProbVote {
                    iteration,
                    header,
                    signature: signature.as_ref().to_vec(),
                });
                if my_node_id == leader {
                    let _ = message_queue_sender.send((my_node_id, vote)).await;
                } else {
                    unicast(private_key, connections, &vote, leader, my_node_id, enable_crypto).await;
                }

            }
            _ => {}
        }
    }

    async fn handle_connection(
        enable_crypto: bool,
        mut stream: TcpStream,
        message_queue_sender: Sender<(NodeId, ProbabilisticSimplexMessage)>,
        public_keys: &HashMap<u32, PublicKey>,
        sample_size: usize,
        n_nodes: usize,
        probabilistic_quorum_size: usize,
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

            let message: ProbabilisticSimplexMessage = match deserialize(&buffer) {
                Ok(msg) => msg,
                Err(_) => {
                    error!("[Node {}] Error deserializing message from Node {}", my_node_id, node_id);
                    return;
                }
            };

            if enable_crypto {
                let mut signature = vec![0; SIGNATURE_BYTES_LENGTH];
                let (payload, signature) = if message.is_vote() {
                    (message.get_vote_header_bytes().expect(&format!("[Node {}] Error reading message signature from Node {}", my_node_id, node_id)),
                     message.get_signature_bytes().unwrap_or_default())
                } else {
                    if stream.read_exact(&mut signature).await.is_err() {
                        error!("[Node {}] Error reading message signature from Node {}", my_node_id, node_id);
                        return;
                    }
                    (buffer, signature.as_ref())
                };
                if !public_key.verify(&payload, &Signature::from_bytes(&signature).unwrap()).is_ok() {
                    continue;
                }
            }
            let _ = message_queue_sender.send((node_id, message)).await;
        }
    }

    async fn handle_propose(
        &mut self,
        propose: ProbPropose,
        sender: NodeId,
        dispatcher_queue_sender: &Sender<Dispatch>,
        reset_timer_sender: &Sender<()>,
        finalize_sender: &Sender<Vec<NotarizedBlock>>
    ) {
        info!("Received propose {}", propose.content.length);
        let leader = Self::get_leader(self.environment.nodes.len(), propose.content.iteration);
        if !self.proposes.contains_key(&propose.content.iteration) && sender == leader {
            let iteration = self.iteration.load(Ordering::Acquire);
            let header = SimplexBlockHeader::from(&propose.content);
            self.proposes.insert(propose.content.iteration, header.clone());
            self.transactions.insert(propose.content.iteration, propose.content.transactions);

            if (propose.content.iteration == iteration + 1 || propose.content.iteration == 1) && (propose.last_notarized_iter == iteration || propose.last_notarized_iter == 0) && propose.last_notarized_cert.len() >= self.quorum_size {
                if self.proposes.contains_key(&propose.last_notarized_iter) {
                    let last_notarized_header = self.proposes.get(&propose.last_notarized_iter).unwrap();

                    if propose.content.iteration > 1 {
                        let serialized_message = match serialize(&last_notarized_header) {
                            Ok(msg) => msg,
                            Err(e) => {
                                error!("Failed to serialize message: {}", e);
                                return;
                            }
                        };

                        let messages: Vec<&[u8]> = (0..propose.last_notarized_cert.len()).map(|_| serialized_message.as_slice()).collect();
                        let signatures: Vec<Signature> = propose.last_notarized_cert.iter().map(
                            |vote_signature| Signature::from_bytes(vote_signature.signature.as_ref()).unwrap()
                        ).collect();
                        let mut keys = Vec::with_capacity(propose.last_notarized_cert.len());
                        for vote_signature in &propose.last_notarized_cert {
                            match self.public_keys.get(&vote_signature.node) {
                                Some(key) => keys.push(*key),
                                None => return,
                            }
                        }

                        if !verify_batch(&messages[..], &signatures[..], &keys[..]).is_ok() {
                            return;
                        }
                        self.votes.insert(last_notarized_header.clone(), propose.last_notarized_cert);
                    }

                    if self.is_extendable(&header) {
                        let vote = Self::create_vote(propose.content.iteration, header.clone());
                        let _ = dispatcher_queue_sender.send(vote).await;
                    };
                    if propose.content.iteration > 1 {
                        self.handle_notarization(propose.last_notarized_iter, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await;
                        self.finalize(iteration, finalize_sender).await;
                        self.handle_iteration_advance(dispatcher_queue_sender, reset_timer_sender, finalize_sender).await;
                    }

                    return;
                }
            }

            if (propose.content.iteration > iteration + 1 && iteration != 1 || propose.content.iteration >= iteration + 1 && iteration == 1) && propose.last_notarized_cert.len() >= self.quorum_size {
                self.propose_certificates.insert(propose.last_notarized_iter, propose.last_notarized_cert);
            }
        }
    }

    async fn handle_vote(
        &mut self,
        vote: ProbVote,
        sender: NodeId,
        dispatcher_queue_sender: &Sender<Dispatch>,
        reset_timer_sender: &Sender<()>,
        finalize_sender: &Sender<Vec<NotarizedBlock>>
    ) {
        info!("Received vote {}", vote.iteration);
        let vote_signatures = self.votes.entry(vote.header).or_insert_with(Vec::new);
        let is_first_vote = !vote_signatures.iter().any(|vote_signature| vote_signature.node == sender);
        if is_first_vote {
            vote_signatures.push(VoteSignature { signature: vote.signature, node: sender });
            info!("{} signatures", vote_signatures.len());
            if vote_signatures.len() == self.quorum_size {
                match self.proposes.get(&vote.iteration) {
                    None => return,
                    Some(header) => {
                        if vote.iteration == self.iteration.load(Ordering::Acquire) {
                            self.handle_notarization(header.iteration, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await;
                            self.handle_iteration_advance(dispatcher_queue_sender, reset_timer_sender, finalize_sender).await;
                        }
                    }
                }
            }
        }
    }

    async fn request(&self, sender: NodeId, dispatcher_queue_sender: &Sender<Dispatch>) {
        info!("Request sent");
        let (last_notarized, _) = self.last_notarized();
        let request = Dispatch::Request(last_notarized.length, sender);
        let _ = dispatcher_queue_sender.send(request).await;
    }

    fn add_genesis_block(&mut self) {
        let genesis_header = SimplexBlockHeader { hash: None, iteration: Self::GENESIS_ITERATION, length: Self::GENESIS_LENGTH, transactions: Vec::new() };
        self.proposes.insert(Self::GENESIS_ITERATION, genesis_header);
        self.transactions.insert(Self::GENESIS_ITERATION, Vec::new());
        let genesis_header = SimplexBlockHeader { hash: None, iteration: Self::GENESIS_ITERATION, length: Self::GENESIS_LENGTH, transactions: Vec::new() };
        self.votes.insert(genesis_header, vec![VoteSignature { signature: Vec::new(), node: 0 }; self.quorum_size]);
    }

    fn last_notarized(&self) -> (&SimplexBlockHeader, &Vec<VoteSignature>) {
        self.votes
            .iter()
            .filter(|(_header, signatures)| signatures.len() >= self.quorum_size)
            .max_by_key(|(header, _)| header.iteration)
            .expect("Impossible scenario")
    }

    fn get_next_block(&self, iteration: Iteration, transactions: Vec<Transaction>) -> SimplexBlock {
        let (last_notarized, _) = self.last_notarized();
        let hash = hash(last_notarized);
        SimplexBlock::new(Some(hash), iteration, last_notarized.length + 1, transactions)
    }

    fn is_extendable(&self, block: &SimplexBlockHeader) -> bool {
        let (last_notarized, _) = self.last_notarized();
        block.hash == Some(hash(last_notarized)) && block.iteration > last_notarized.iteration && block.length == last_notarized.length + 1
    }

    fn get_notarized(&self, iteration: Iteration) -> Option<(&SimplexBlockHeader, &Vec<VoteSignature>)> {
        if iteration >= self.iteration.load(Ordering::Acquire) {
            return None
        }
        self.votes
            .iter()
            .find(|(header, signatures)|
                signatures.len() >= self.quorum_size &&
                header.iteration == iteration &&
                self.proposes.get(&iteration).is_some()
            )
    }

    fn is_missing(&self, length: u32, iteration: Iteration) -> bool {
        let (last_notarized, _) = self.last_notarized();
        last_notarized.length < length || (last_notarized.length == length && iteration != last_notarized.iteration)
    }

    async fn handle_notarization(
        &mut self,
        iteration: Iteration,
        dispatcher_queue_sender: &Sender<Dispatch>,
        reset_timer_sender: &Sender<()>,
        finalize_sender: &Sender<Vec<NotarizedBlock>>,
    ) {
        self.iteration.fetch_add(1, Ordering::AcqRel);
        let _ = reset_timer_sender.send(()).await;
    }

    async fn finalize(&mut self, iteration: Iteration, finalize_sender: &Sender<Vec<NotarizedBlock>>) {
        let mut blocks_to_be_finalized: Vec<NotarizedBlock> = Vec::new();
        if self.finalized_height + 1 <= iteration {
            for iter in self.finalized_height + 1 ..= iteration {
                if self.finalization_timestamps.get_mut(&iter).is_some() {
                    self.finalization_timestamps.get_mut(&iter).unwrap().finalization = Some(SystemTime::now());
                }
                let header = self.proposes.get(&iter).unwrap().clone();
                let signatures = self.votes.get(&header).unwrap().clone();
                let transactions = self.transactions.remove(&iter).unwrap();
                blocks_to_be_finalized.push(NotarizedBlock { header, signatures, transactions });
            }

            self.finalized_height = iteration;
            self.blocks_finalized += blocks_to_be_finalized.len();
            let _ = finalize_sender.send(blocks_to_be_finalized.into_iter().collect()).await;
        }
    }

    async fn get_missing(&self, _from_length: u32, _sender: NodeId, _dispatcher_queue_sender: &Sender<Dispatch>) {
        //TODO
   }
}