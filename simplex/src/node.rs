use crate::block::{HashedNotarizedBlock, HashedSimplexBlock, SimplexBlock};
use crate::blockchain::Blockchain;
use crate::message::{Finalize, Propose, Reply, Request, SimplexMessage, Timeout, View, Vote};
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde_json::to_string;
use sha1::{Digest, Sha1};
use shared::connection::{accept_connections, broadcast, connect, notify, unicast, NodeId, Signature};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{self, Duration};

const INITIAL_ITERATION: u32 = 1;
const ITERATION_TIME: u64 = 10;
const MESSAGE_CHANNEL_SIZE: usize = 50;

pub struct SimplexNode {
    environment: Environment,
    iteration: Arc<AtomicU32>,
    is_timeout: Arc<AtomicBool>,
    proposes: HashMap<u32, bool>,
    votes: HashMap<SimplexBlock, Vec<(SimplexMessage, Signature, NodeId)>>,
    timeouts: HashMap<u32, Vec<NodeId>>,
    finalizes: HashMap<u32, Vec<NodeId>>,
    blockchain: Blockchain,
    transaction_generator: TransactionGenerator,
    public_keys: HashMap<u32, Vec<u8>>,
    private_key: Ed25519KeyPair,
}

enum IterationAdvance {
    Timeout,
    Notarization,
}

impl SimplexNode {
    pub fn new(environment: Environment, public_keys: HashMap<u32, Vec<u8>>, private_key: Ed25519KeyPair) -> Self {
        let my_node_id = environment.my_node.id;
        SimplexNode {
            environment,
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
                let (tx, rx) = mpsc::channel::<(NodeId, SimplexMessage, Signature)>(MESSAGE_CHANNEL_SIZE);
                let (timer_tx, timer_rx) = mpsc::channel::<()>(1);
                let (timeout_tx, timeout_rx) = mpsc::channel::<Timeout>(1);
                let (reset_tx,reset_rx) = mpsc::channel::<()>(1);
                let sender = Arc::new(tx);
                connect(self.environment.my_node.id, &self.environment.nodes, &self.public_keys, sender).await;
                let mut connections = accept_connections(self.environment.my_node.id, &self.environment.nodes, listener).await;
                self.start_iteration_timer(timer_rx, timeout_tx, reset_rx).await;
                self.execute_protocol(rx, timer_tx, timeout_rx, reset_tx, &mut connections).await;
            }
            Err(_) => { eprintln!("[Node {}] Failed to bind local port", self.environment.my_node.id) }
        };
    }

    async fn start_iteration_timer(&self, mut timer_rx: Receiver<()>, timeout_tx: Sender<Timeout>, mut reset_rx: Receiver<()>) {
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
                            match timeout_tx.send(timeout).await {
                                Ok(_) => {}
                                Err(_) => {}
                            };
                            timer_rx.recv().await;
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

    async fn execute_protocol(
        &mut self,
        mut message_queue_receiver: Receiver<(NodeId, SimplexMessage, Signature)>,
        timer_tx: Sender<()>,
        mut timeout_rx: Receiver<Timeout>,
        reset_tx: Sender<()>,
        connections: &mut Vec<TcpStream>
    ) {
        let (iter_tx, mut iter_rx) = mpsc::channel::<IterationAdvance>(1);
        match iter_tx.send(IterationAdvance::Notarization).await {
            Ok(_) => {}
            Err(_) => {}
        };
        loop {
            tokio::select! {
                Some(timeout) = timeout_rx.recv() => {
                    if timeout.next_iter - 1 == self.iteration.load(Ordering::SeqCst) {
                        let message = SimplexMessage::Timeout(timeout);
                        broadcast(&self.private_key, connections, message).await;
                    }
                }

                Some(_) = iter_rx.recv() => {
                    let iteration = self.iteration.load(Ordering::SeqCst);
                    let leader = self.get_leader(iteration);
                    self.is_timeout.store(false, Ordering::SeqCst);
                    self.timeouts.retain(|iter, _| *iter > iteration);
                    println!("----------------------");
                    println!("----------------------");
                    println!("Leader is node {} | Iteration: {}", leader, iteration);
                    self.blockchain.print();
                    if leader == self.environment.my_node.id {
                        self.propose(connections).await;
                    }
                }

                Some((sender, message, signature)) = message_queue_receiver.recv() => {
                    let iteration = self.iteration.load(Ordering::SeqCst);
                    let leader = self.get_leader(iteration);
                    match message {
                        SimplexMessage::Propose(propose) => self.handle_propose(propose, sender, leader, iteration, connections).await,
                        SimplexMessage::Vote(vote) => self.handle_vote(vote.clone(), SimplexMessage::Vote(vote.clone()), sender, signature, iter_tx.clone(), reset_tx.clone(), connections).await,
                        SimplexMessage::Timeout(timeout) => self.handle_timeout(timeout, sender, iteration, iter_tx.clone(), timer_tx.clone()).await,
                        SimplexMessage::Finalize(finalize) => self.handle_finalize(finalize, sender, connections).await,
                        SimplexMessage::View(view) => self.handle_view(view, connections, sender).await,
                        SimplexMessage::Request(request) => self.handle_request(request, connections, sender).await,
                        SimplexMessage::Reply(reply) => self.handle_reply(reply, iter_tx.clone(), reset_tx.clone(), connections).await,
                    }
                }
            }
        }
    }

    fn get_leader(&self, iteration: u32) -> u32 {
        if iteration == 0 {
            return 0;
        }
        let mut hasher = Sha1::new();
        hasher.update(&iteration.to_le_bytes());
        let hash = hasher.finalize();
        let hash_u64 = u64::from_le_bytes(hash[0..8].try_into().expect("Invalid slice length"));
        (hash_u64 % self.environment.nodes.len() as u64) as u32
    }

    async fn propose(&mut self, connections: &mut Vec<TcpStream>) {
        let iteration = self.iteration.load(Ordering::SeqCst);
        let transactions = self.transaction_generator.generate(self.environment.my_node.id);
        let block = self.blockchain.get_next_block(iteration, transactions);
        let message = SimplexMessage::Propose(Propose { content: block });
        broadcast(&self.private_key, connections, message).await;
    }

    async fn handle_propose(&mut self, propose: Propose, sender: u32, leader: u32, iteration: u32, connections: &mut Vec<TcpStream>) {
        if !self.proposes.contains_key(&propose.content.iteration) && sender == leader && iteration == propose.content.iteration {
            self.proposes.insert(propose.content.iteration, true);
            if self.blockchain.is_extendable(&propose.content) {
                let vote = SimplexMessage::Vote(Vote { content: propose.content });
                broadcast(&self.private_key, connections, vote).await;
            }
        }
    }

    async fn handle_vote(
        &mut self,
        vote: Vote,
        message: SimplexMessage,
        sender: NodeId,
        signature: Signature,
        iter_tx: Sender<IterationAdvance>,
        reset_tx: Sender<()>,
        connections: &mut Vec<TcpStream>,
    ) {
        let iteration = vote.content.iteration;
        let vote_signatures = self.votes.entry(vote.content.clone()).or_insert_with(Vec::new);
        let is_first_vote = !vote_signatures.iter().any(|(_, _, node_id)| *node_id == sender);
        if is_first_vote {
            vote_signatures.push((message, signature, sender));
            if vote_signatures.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                let is_added = self.blockchain.add_block(vote.content.clone(), vote_signatures.to_vec());
                if iteration == self.iteration.load(Ordering::SeqCst) && is_added {
                    self.iteration.fetch_add(1, Ordering::SeqCst);
                    match reset_tx.send(()).await {
                        Ok(_) => {}
                        Err(_) => {}
                    };
                    match iter_tx.send(IterationAdvance::Notarization).await {
                        Ok(_) => {}
                        Err(_) => {}
                    };
                    if !self.is_timeout.load(Ordering::SeqCst) {
                        let finalize = SimplexMessage::Finalize(Finalize { iter: iteration });
                        broadcast(&self.private_key, connections, finalize).await;
                    }
                    let view = SimplexMessage::View(View { last_notarized: HashedNotarizedBlock { block: HashedSimplexBlock::from(vote.content), signatures: vote_signatures.to_vec() } });
                    notify(&self.private_key, connections, view, &self.environment.my_node.host, self.environment.my_node.port).await;
                }

                if !is_added {
                    self.request(sender, connections).await;
                }
            }
        }
    }

    async fn handle_timeout(&mut self, timeout: Timeout, sender: NodeId, iteration: u32, iter_tx: Sender<IterationAdvance>, timer_tx: Sender<()>) {
        if timeout.next_iter > iteration {
            let timeouts = self.timeouts.entry(timeout.next_iter).or_insert_with(Vec::new);
            let is_first_timeout = !timeouts.iter().any(|node_id| *node_id == sender);
            if is_first_timeout {
                timeouts.push(sender);
                if timeouts.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                    self.iteration.store(timeout.next_iter, Ordering::SeqCst);
                    match timer_tx.send(()).await {
                        Ok(_) => {}
                        Err(_) => {}
                    };
                    match iter_tx.send(IterationAdvance::Timeout).await {
                        Ok(_) => {}
                        Err(_) => {}
                    };
                }
            }
        }
    }

    async fn handle_finalize(&mut self, finalize: Finalize, sender: NodeId, connections: &mut Vec<TcpStream>) {
        let finalizes = self.finalizes.entry(finalize.iter).or_insert_with(Vec::new);
        let is_first_finalize = !finalizes.iter().any(|node_id| *node_id == sender);
        if is_first_finalize {
            finalizes.push(sender);
            if finalizes.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                if let Some(_) = self.blockchain.get_block(finalize.iter) {
                    self.blockchain.finalize(finalize.iter);
                    // clear data
                    self.proposes.retain(|iteration, _| *iteration > finalize.iter);
                    self.votes.retain(|_, signatures| signatures.len() >= self.environment.nodes.len() * 2 / 3 + 1);
                    self.finalizes.retain(|iteration, _| *iteration > finalize.iter);
                } else {
                    self.blockchain.add_to_be_finalized(finalize.iter);
                    self.request(sender, connections).await;
                }
            }
        }
    }

    async fn handle_view(
        &mut self,
        view: View,
        connections: &mut Vec<TcpStream>,
        sender: NodeId,
    ) {
        // check view block signatures and check for missing blocks
        if self.blockchain.is_missing(view.last_notarized.block.length) && view.last_notarized.signatures.len() >= self.environment.nodes.len() * 2 / 3 + 1 {
            for (message, signature, node_id) in view.last_notarized.signatures.iter() {
                match self.public_keys.get(node_id) {
                    None => return,
                    Some(key) => {
                        let public_key = UnparsedPublicKey::new(&ED25519, key);
                        let serialized_message = match to_string(&message) {
                            Ok(json) => json,
                            Err(e) => {
                                eprintln!("Failed to serialize message: {}", e);
                                return;
                            }
                        };
                        let bytes = serialized_message.as_bytes();
                        match public_key.verify(bytes, signature) {
                            Ok(_) => {}
                            Err(_) => return
                        }
                    }
                }
            }
            self.request(sender, connections).await;
        }
    }

    async fn handle_request(&mut self, request: Request, connections: &mut Vec<TcpStream>, sender: NodeId) {
        let missing = self.blockchain.get_missing(request.last_notarized.length);
        if let Some(sender_node) = self.environment.nodes.iter().find(|node| node.id == sender) {
            if let Some(stream) = connections.iter_mut().find(|stream | stream.peer_addr().unwrap().to_string() == format!("{}:{}", sender_node.host, sender_node.port)) {
                unicast(&self.private_key, stream, SimplexMessage::Reply(Reply { blocks: missing })).await;
            }
        }
    }

    async fn handle_reply(
        &mut self,
        reply: Reply,
        iter_tx: Sender<IterationAdvance>,
        reset_tx: Sender<()>,
        connections: &mut Vec<TcpStream>,
    ) {
        for notarized in reply.blocks {
            if self.blockchain.is_missing(notarized.block.length) && notarized.signatures.len() >= self.environment.nodes.len() * 2 / 3 + 1 {
                for (message, signature, node_id) in notarized.signatures.iter() {
                    match self.public_keys.get(node_id) {
                        Some(key) => {
                            let public_key = UnparsedPublicKey::new(&ED25519, key);
                            let serialized_message = match to_string(&message) {
                                Ok(json) => json,
                                Err(e) => {
                                    eprintln!("Failed to serialize message: {}", e);
                                    return;
                                }
                            };
                            let bytes = serialized_message.as_bytes();
                            match public_key.verify(bytes, signature) {
                                Ok(_) => {}
                                Err(_) => continue
                            }
                        }
                        None => return,
                    }
                }
                let iteration = notarized.block.iteration;
                let is_added = self.blockchain.add_block(notarized.block.clone(), notarized.signatures.clone());
                if iteration == self.iteration.load(Ordering::SeqCst) && is_added {
                    self.iteration.fetch_add(1, Ordering::SeqCst);
                    match reset_tx.send(()).await {
                        Ok(_) => {}
                        Err(_) => {}
                    };
                    match iter_tx.send(IterationAdvance::Notarization).await {
                        Ok(_) => {}
                        Err(_) => {}
                    };
                    if !self.is_timeout.load(Ordering::SeqCst) {
                        let finalize = SimplexMessage::Finalize(Finalize { iter: iteration });
                        broadcast(&self.private_key, connections, finalize).await;
                    }
                    let view = SimplexMessage::View(View { last_notarized: HashedNotarizedBlock { block: HashedSimplexBlock::from(notarized.block), signatures: notarized.signatures } });
                    notify(&self.private_key, connections, view, &self.environment.my_node.host, self.environment.my_node.port).await;
                }
            }
        }
    }

    async fn request(&self, sender: NodeId, connections: &mut Vec<TcpStream>) {
        if let Some(sender_node) = self.environment.nodes.iter().find(|node| node.id == sender) {
            if let Some(stream) = connections.iter_mut().find(|stream| stream.peer_addr().unwrap().to_string() == format!("{}:{}", sender_node.host, sender_node.port)) {
                unicast(&self.private_key, stream, SimplexMessage::Request(Request { last_notarized: HashedSimplexBlock::from(self.blockchain.last()) })).await;
            }
        }
    }
}
