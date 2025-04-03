use crate::block::{HashedSimplexBlock, VoteSignature};
use crate::blockchain::Blockchain;
use crate::connection::{broadcast, broadcast_to_sample, connect, unicast};
use crate::message::{Finalize, Propose, Reply, Request, SimplexMessage, Timeout, Vote};
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde_json::to_string;
use sha2::{Digest, Sha256};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{self, Duration};
use shared::vrf::{vrf_prove, vrf_verify};

const INITIAL_ITERATION: u32 = 1;
const ITERATION_TIME: u64 = 10;
const MESSAGE_CHANNEL_SIZE: usize = 100;
const CONST_O: f32 = 1.7;
const CONST_L: f32 = 1.0;

pub struct SimplexNode {
    environment: Environment,
    quorum_size: usize,
    sample_size: usize,
    iteration: Arc<AtomicU32>,
    is_timeout: Arc<AtomicBool>,
    proposes: HashMap<u32, Propose>,
    votes: HashMap<HashedSimplexBlock, Vec<VoteSignature>>,
    timeouts: HashMap<u32, Vec<u32>>,
    finalizes: HashMap<u32, Vec<u32>>,
    blockchain: Blockchain,
    transaction_generator: TransactionGenerator,
    public_keys: HashMap<u32, Vec<u8>>,
    private_key: Ed25519KeyPair,
}

impl SimplexNode {
    pub fn new(environment: Environment, public_keys: HashMap<u32, Vec<u8>>, private_key: Ed25519KeyPair) -> Self {
        let my_node_id = environment.my_node.id;
        let n = environment.nodes.len();
        SimplexNode {
            environment,
            quorum_size: (CONST_L * (n as f32).sqrt()).floor() as usize,
            sample_size: (CONST_O * CONST_L * (n as f32).sqrt()).floor() as usize,
            iteration: Arc::new(AtomicU32::new(INITIAL_ITERATION)),
            is_timeout: Arc::new(AtomicBool::new(false)),
            proposes: HashMap::new(),
            votes: HashMap::new(),
            timeouts: HashMap::new(),
            finalizes: HashMap::new(),
            blockchain: Blockchain::new(my_node_id),
            transaction_generator: TransactionGenerator::new(),
            public_keys,
            private_key
        }
    }

    pub async fn start_simplex(mut self) {
        let address = format!("{}:{}", self.environment.my_node.host, self.environment.my_node.port);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let (tx, rx) = mpsc::channel::<(u32, SimplexMessage)>(MESSAGE_CHANNEL_SIZE);
                let (timeout_tx, timeout_rx) = mpsc::channel::<Timeout>(1);
                let (reset_tx,reset_rx) = mpsc::channel::<()>(1);
                let sender = Arc::new(tx);
                let mut connections = connect(self.environment.my_node.id, &self.environment.nodes, &self.public_keys, sender, listener).await;
                self.start_iteration_timer(timeout_tx, reset_rx).await;
                self.execute_protocol(rx, timeout_rx, reset_tx, &mut connections).await;
            }
            Err(_) => { eprintln!("[Node {}] Failed to bind local port", self.environment.my_node.id) }
        };
    }

    async fn start_iteration_timer(&self, timeout_tx: Sender<Timeout>, mut reset_rx: Receiver<()>) {
        let iteration_counter = self.iteration.clone();
        let is_timeout = self.is_timeout.clone();
        let mut timer = time::interval(Duration::from_secs(ITERATION_TIME));
        timer.tick().await;
        tokio::spawn(async move {
            loop {
                let iteration = iteration_counter.load(Ordering::SeqCst);
                tokio::select! {
                    _ = timer.tick() => {
                        if iteration == iteration_counter.load(Ordering::SeqCst) {
                            is_timeout.store(true, Ordering::SeqCst);
                            let timeout = Timeout { next_iter: iteration + 1 };
                            let _ = timeout_tx.send(timeout).await;
                            reset_rx.recv().await;
                            timer = time::interval(Duration::from_secs(ITERATION_TIME));
                            timer.tick().await;
                        }
                    }
                    _ = reset_rx.recv() => {
                        timer = time::interval(Duration::from_secs(ITERATION_TIME));
                        timer.tick().await;
                    }
                }
            }
        });
    }

    async fn handle_iteration_advance(&mut self, connections: &mut Vec<TcpStream>) {
        loop {
            let iteration = self.iteration.load(Ordering::SeqCst);
            let leader = Self::get_leader(self.environment.nodes.len(), iteration);
            self.is_timeout.store(false, Ordering::SeqCst);
            self.timeouts.retain(|iter, _| *iter > iteration);
            println!("----------------------");
            println!("----------------------");
            println!("Leader is node {} | Iteration: {}", leader, iteration);
            self.blockchain.print();

            if leader == self.environment.my_node.id {
                let transactions = self.transaction_generator.generate(self.environment.my_node.id);
                let block = self.blockchain.get_next_block(iteration, transactions);
                println!("Proposed {}", block.length);
                let message = SimplexMessage::Propose(Propose { content: block });
                broadcast(&self.private_key, connections, message).await;
            }

            match self.proposes.get(&iteration) {
                None => {}
                Some(propose) => {
                    if !self.is_timeout.load(Ordering::SeqCst) && self.blockchain.is_extendable(&propose.content) {
                        let block = HashedSimplexBlock::from(&propose.content);
                        let serialized_message = match to_string(&block) {
                            Ok(json) => json,
                            Err(e) => {
                                eprintln!("Failed to serialize hashed block: {}", e);
                                return;
                            }
                        };
                        let serialized_bytes = serialized_message.as_bytes();
                        let n = self.environment.nodes.len();
                        let next_leader = Self::get_leader(self.environment.nodes.len(), iteration + 1);
                        let (sample, proof) = vrf_prove(&self.private_key, &format!("{}vote", iteration), self.sample_size, n as u32, next_leader);
                        let vote = SimplexMessage::Vote(Vote {
                            iteration, header: block,
                            signature: Vec::from(self.private_key.sign(serialized_bytes).as_ref()),
                            sample: sample.clone().into_iter().collect(),
                            proof
                        });
                        broadcast_to_sample(&self.private_key, connections, vote, sample.into_iter().collect()).await;
                        println!("Voted for {}", propose.content.length);
                    }
                }
            }

            match self.timeouts.get(&(iteration + 1)) {
                None => { },
                Some(timeouts) => {
                    if timeouts.len() == self.quorum_size {
                        self.iteration.store(iteration + 1, Ordering::SeqCst);
                        continue
                    }
                }
            }

            match self.blockchain.check_for_possible_notarization(iteration) {
                None => break,
                Some(notarized) => {
                    self.blockchain.notarize(
                        notarized.block.clone(),
                        notarized.transactions,
                        notarized.signatures.clone()
                    ).await;
                    self.iteration.fetch_add(1, Ordering::SeqCst);
                    if !self.is_timeout.load(Ordering::SeqCst) {
                        let n = self.environment.nodes.len();
                        let next_leader = Self::get_leader(self.environment.nodes.len(), iteration + 1);
                        let (sample, proof) = vrf_prove(&self.private_key, &format!("{}finalize", iteration), self.sample_size, n as u32, next_leader);
                        let finalize = SimplexMessage::Finalize(Finalize { iter: iteration, sample: sample.clone().into_iter().collect(), proof});
                        broadcast_to_sample(&self.private_key, connections, finalize, sample.into_iter().collect()).await;
                    }
                }
            }
        }
    }

    async fn execute_protocol(
        &mut self,
        mut message_queue_receiver: Receiver<(u32, SimplexMessage)>,
        mut timeout_rx: Receiver<Timeout>,
        reset_tx: Sender<()>,
        connections: &mut Vec<TcpStream>
    ) {
        self.handle_iteration_advance(connections).await;
        loop {
            tokio::select! {
                Some(timeout) = timeout_rx.recv() => {
                    if timeout.next_iter - 1 == self.iteration.load(Ordering::SeqCst) {
                        let message = SimplexMessage::Timeout(timeout);
                        broadcast(&self.private_key, connections, message).await;
                    }
                }

                Some((sender, message)) = message_queue_receiver.recv() => {
                    match message {
                        SimplexMessage::Propose(propose) => self.handle_propose(propose, sender, connections).await,
                        SimplexMessage::Vote(vote) => self.handle_vote(vote.clone(), sender, reset_tx.clone(), connections).await,
                        SimplexMessage::Timeout(timeout) => self.handle_timeout(timeout, sender, reset_tx.clone(), connections).await,
                        SimplexMessage::Finalize(finalize) => self.handle_finalize(finalize, sender, connections).await,
                        SimplexMessage::Request(request) => self.handle_request(request, connections, sender).await,
                        SimplexMessage::Reply(reply) => self.handle_reply(reply, reset_tx.clone(), connections).await,
                    }
                }
            }
        }
    }

    fn get_leader(n_nodes: usize, iteration: u32) -> u32 {
        if iteration == 0 {
            return 0;
        }
        let mut hasher = Sha256::new();
        hasher.update(&iteration.to_le_bytes());
        let hash = hasher.finalize();
        let hash_u64 = u64::from_le_bytes(hash[0..8].try_into().expect("Invalid slice length"));
        (hash_u64 % n_nodes as u64) as u32
    }

    async fn handle_propose(&mut self, propose: Propose, sender: u32, connections: &mut Vec<TcpStream>) {
        println!("Received propose {}", propose.content.length);
        let leader = Self::get_leader(self.environment.nodes.len(), propose.content.iteration);
        if !self.proposes.contains_key(&propose.content.iteration) && sender == leader {
            self.proposes.insert(propose.content.iteration, propose.clone());
            let iteration = self.iteration.load(Ordering::SeqCst);
            let is_extendable = self.blockchain.is_extendable(&propose.content);
            if is_extendable && iteration == propose.content.iteration && !self.is_timeout.load(Ordering::SeqCst) {
                let block = HashedSimplexBlock::from(&propose.content);
                let serialized_message = match to_string(&block) {
                    Ok(json) => json,
                    Err(e) => {
                        eprintln!("Failed to serialize hashed block: {}", e);
                        return;
                    }
                };
                let serialized_bytes = serialized_message.as_bytes();
                let n = self.environment.nodes.len();
                let next_leader = Self::get_leader(self.environment.nodes.len(), propose.content.iteration + 1);
                let (sample, proof) = vrf_prove(&self.private_key, &format!("{}vote", iteration), self.sample_size, n as u32, next_leader);
                println!("Voted for {}", propose.content.length);
                let vote = SimplexMessage::Vote(Vote {
                    iteration,
                    header: block,
                    signature: Vec::from(self.private_key.sign(serialized_bytes).as_ref()),
                    sample: sample.clone().into_iter().collect(),
                    proof
                });
                broadcast_to_sample(&self.private_key, connections, vote, sample.into_iter().collect()).await;
            }
            if !is_extendable {
                self.request(sender, connections).await;
            }
        }
    }

    async fn handle_vote(
        &mut self,
        vote: Vote,
        sender: u32,
        reset_tx: Sender<()>,
        connections: &mut Vec<TcpStream>,
    ) {
        if !vote.sample.contains(&self.environment.my_node.id) {
            return;
        }
        println!("Received vote {}", vote.iteration);
        match self.public_keys.get(&sender) {
            Some(key) => {
                let n = self.environment.nodes.len();
                let leader = Self::get_leader(n, vote.iteration + 1);
                if !vrf_verify(key, &format!("{}vote", vote.iteration), self.sample_size, n as u32, leader, vote.sample.into_iter().collect(), &vote.proof) {
                    return;
                }
                let public_key = UnparsedPublicKey::new(&ED25519, key);
                let serialized_message = match to_string(&vote.header) {
                    Ok(json) => json,
                    Err(e) => {
                        eprintln!("Failed to serialize hashed block: {}", e);
                        return;
                    }
                };
                let serialized_bytes = serialized_message.as_bytes();
                match public_key.verify(serialized_bytes, vote.signature.as_ref()) {
                    Ok(_) => {}
                    Err(_) => return
                };
            },
            None => return
        }

        let vote_signatures = self.votes.entry(vote.header).or_insert_with(Vec::new);
        let is_first_vote = !vote_signatures.iter().any(|vote_signature| vote_signature.node == sender);
        if is_first_vote {
            vote_signatures.push(VoteSignature { signature: vote.signature, node: sender });
            println!("{} signatures", vote_signatures.len());
            let iteration = self.iteration.load(Ordering::SeqCst);
            if vote_signatures.len() == self.quorum_size {
                match self.proposes.get(&vote.iteration) {
                    None => return,
                    Some(block) => {
                        let is_timeout = self.is_timeout.load(Ordering::SeqCst);
                        if vote.iteration == iteration && !is_timeout {
                            let is_notarized = self.blockchain.notarize(
                                HashedSimplexBlock::from(&block.content),
                                block.content.transactions.clone(),
                                vote_signatures.to_vec()
                            ).await;

                            if is_notarized {
                                self.iteration.fetch_add(1, Ordering::SeqCst);
                                if !is_timeout {
                                    let n = self.environment.nodes.len();
                                    let leader = Self::get_leader(self.environment.nodes.len(), iteration + 1);
                                    let (sample, proof) = vrf_prove(&self.private_key, &format!("{}finalize", iteration), self.sample_size, n as u32, leader);
                                    let finalize = SimplexMessage::Finalize(Finalize { iter: iteration, sample: sample.clone().into_iter().collect(), proof});
                                    broadcast_to_sample(&self.private_key, connections, finalize, sample.into_iter().collect()).await;
                                }
                                self.handle_iteration_advance(connections).await;
                                let _ = reset_tx.send(()).await;
                            } else {
                                self.request(sender, connections).await;
                            }
                        } else {
                            self.blockchain.add_to_be_notarized(
                                HashedSimplexBlock::from(&block.content),
                                block.content.transactions.clone(),
                                vote_signatures.to_vec()
                            )
                        }
                    }
                }
            }
        }
    }

    async fn handle_timeout(&mut self, timeout: Timeout, sender: u32, reset_tx: Sender<()>, connections: &mut Vec<TcpStream>) {
        println!("Received timeout {}", timeout.next_iter);
        let timeouts = self.timeouts.entry(timeout.next_iter).or_insert_with(Vec::new);
        let is_first_timeout = !timeouts.iter().any(|node_id| *node_id == sender);
        if is_first_timeout {
            timeouts.push(sender);
            println!("{} matching timeouts for iter {}", timeouts.len(), timeout.next_iter);
            if timeouts.len() == self.environment.nodes.len() * 2 / 3 + 1 && timeout.next_iter == self.iteration.load(Ordering::SeqCst) + 1 {
                self.iteration.store(timeout.next_iter, Ordering::SeqCst);
                self.handle_iteration_advance(connections).await;
                let _ = reset_tx.send(()).await;
            }
        }
    }

    async fn handle_finalize(&mut self, finalize: Finalize, sender: u32, connections: &mut Vec<TcpStream>) {
        println!("Received finalize {}", finalize.iter);
        if !finalize.sample.contains(&self.environment.my_node.id)  {
            return;
        }
        match self.public_keys.get(&sender) {
            Some(key) => {
                let n = self.environment.nodes.len();
                let leader = Self::get_leader(n, finalize.iter + 1);
                if !vrf_verify(key, &format!("{}finalize", finalize.iter), self.sample_size, n as u32, leader, finalize.sample.into_iter().collect(), &finalize.proof) {
                    return;
                }
            },
            None => return
        }
        let finalizes = self.finalizes.entry(finalize.iter).or_insert_with(Vec::new);
        let is_first_finalize = !finalizes.iter().any(|node_id| *node_id == sender);
        if is_first_finalize {
            finalizes.push(sender);
            if finalizes.len() == self.quorum_size {
                if self.blockchain.has_block(finalize.iter) {
                    if !self.environment.test_flag {
                        self.blockchain.finalize(finalize.iter).await;
                    }
                    self.proposes.retain(|iteration, _| *iteration > finalize.iter);
                    self.votes.retain(|_, signatures| signatures.len() < self.quorum_size);
                    self.finalizes.retain(|iteration, _| *iteration > finalize.iter);
                } else {
                    if !self.environment.test_flag {
                        self.blockchain.add_to_be_finalized(finalize.iter);
                    }
                    self.request(sender, connections).await;
                }
            }
        }
    }

    async fn handle_request(&mut self, request: Request, connections: &mut Vec<TcpStream>, sender: u32) {
        println!("Request received");
        let missing = self.blockchain.get_missing(request.last_notarized_length, request.curr_iteration).await;
        if missing.is_empty() {
            return
        }
        if let Some(sender_node) = self.environment.nodes.iter().find(|node| node.id == sender) {
            if let Some(stream) = connections.get_mut(sender_node.id as usize) {
                unicast(&self.private_key, stream, SimplexMessage::Reply(Reply { blocks: missing })).await;
                println!("Reply send");
            }
        }
    }

    async fn handle_reply(
        &mut self,
        reply: Reply,
        reset_tx: Sender<()>,
        connections: &mut Vec<TcpStream>,
    ) {
        println!("Received Reply {:?}", reply.blocks);
        if reply.blocks.is_empty() {
            return;
        }

        let mut is_reset = false;
        for notarized in reply.blocks {
            if self.blockchain.is_missing(notarized.block.length, notarized.block.iteration) && notarized.signatures.len() >= self.quorum_size {
                let transactions_data = to_string(&notarized.transactions).expect("Failed to serialize Block transactions");
                let mut hasher = Sha256::new();
                hasher.update(transactions_data.as_bytes());
                let hashed_transactions = hasher.finalize().to_vec();
                if hashed_transactions != notarized.block.transactions {
                    break;
                }

                for vote_signature in notarized.signatures.iter() {
                    match self.public_keys.get(&vote_signature.node) {
                        Some(key) => {
                            let public_key = UnparsedPublicKey::new(&ED25519, key);
                            let serialized_message = match to_string(&notarized.block) {
                                Ok(json) => json,
                                Err(e) => {
                                    eprintln!("Failed to serialize message: {}", e);
                                    break;
                                }
                            };
                            let bytes = serialized_message.as_bytes();
                            match public_key.verify(bytes, vote_signature.signature.as_ref()) {
                                Ok(_) => { }
                                Err(_) => {
                                    println!("Reply Signature verification failed");
                                    break;
                                }
                            }
                        }
                        None => break,
                    }
                }

                let iteration = self.iteration.load(Ordering::SeqCst);
                if notarized.block.iteration == iteration {
                    let is_notarized = self.blockchain.notarize(notarized.block.clone(), notarized.transactions.clone(), notarized.signatures.clone()).await;
                    if is_notarized {
                        is_reset = true;
                        self.iteration.fetch_add(1, Ordering::SeqCst);
                        if !self.is_timeout.load(Ordering::SeqCst) {
                            let n = self.environment.nodes.len();
                            let leader = Self::get_leader(self.environment.nodes.len(), iteration + 1);
                            let (sample, proof) = vrf_prove(&self.private_key, &format!("{}finalize", iteration), self.sample_size, n as u32, leader);
                            let finalize = SimplexMessage::Finalize(Finalize { iter: iteration, sample: sample.clone().into_iter().collect(), proof});
                            broadcast_to_sample(&self.private_key, connections, finalize, sample.into_iter().collect()).await;
                        }
                        self.handle_iteration_advance(connections).await;
                    } else {
                        self.blockchain.add_to_be_notarized(notarized.block, notarized.transactions, notarized.signatures);
                    }
                }
            }
        }
        if is_reset {
            let _ = reset_tx.send(()).await;
        }
    }

    async fn request(&self, sender: u32, connections: &mut Vec<TcpStream>) {
        if let Some(sender_node) = self.environment.nodes.iter().find(|node| node.id == sender) {
            if let Some(stream) = connections.get_mut(sender_node.id as usize) {
                unicast(&self.private_key, stream, SimplexMessage::Request(Request { last_notarized_length: self.blockchain.last_notarized_length(), curr_iteration: self.iteration.load(Ordering::SeqCst) })).await;
                println!("Request send");
            }
        }
    }
}
