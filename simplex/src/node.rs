use crate::block::SimplexBlock;
use crate::blockchain::Blockchain;
use crate::message::{Finalize, Propose, Reply, Request, SimplexMessage, Timeout, View, Vote};
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde_json::to_string;
use shared::connection::{accept_connections, broadcast, connect, unicast, NodeId, Signature};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use sha1::{Digest, Sha1};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{self, Duration};

const INITIAL_ITERATION: u32 = 1;
const ITERATION_TIME: u64 = 3;
const MESSAGE_CHANNEL_SIZE: usize = 50;

type HashedBlock = Vec<u8>;

pub struct SimplexNode {
    environment: Environment,
    iteration: Arc<AtomicU32>,
    blocks: HashMap<u32, bool>,
    votes: HashMap<HashedBlock, Vec<(SimplexMessage, NodeId, Signature)>>,
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
            blocks: HashMap::new(),
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
                let sender = Arc::new(tx);
                connect(self.environment.my_node.id, &self.environment.nodes, &self.public_keys, sender).await;
                let mut connections = accept_connections(self.environment.my_node.id, &self.environment.nodes, listener).await;

                let (reset_tx, reset_rx) = mpsc::channel(1);
                let (iter_tx, iter_rx) = mpsc::channel(1);
                self.start_iteration_timer(reset_rx, iter_tx).await;

                self.execute_protocol(iter_rx, reset_tx, rx, &mut connections).await;
            }
            Err(_) => { eprintln!("[Node {}] Failed to bind local port", self.environment.my_node.id) }
        };
    }


    async fn start_iteration_timer(&self, mut reset_rx: Receiver<u32>, iter_tx: Sender<IterationAdvance>) {
        let iteration_counter = self.iteration.clone();
        let mut timer = time::interval(Duration::from_secs(ITERATION_TIME));
        timer.tick().await;
        tokio::spawn(async move {
            loop {
                let iteration = iteration_counter.load(Ordering::SeqCst);
                tokio::select! {
                    _ = timer.tick() => {
                        if iteration == iteration_counter.load(Ordering::SeqCst) {
                            if let Err(_) = iter_tx.send(IterationAdvance::Timeout).await {
                                return;
                            }
                            reset_rx.recv().await;
                            iteration_counter.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    Some(next_iteration) = reset_rx.recv() => {
                        if next_iteration != 0 {
                            iteration_counter.store(next_iteration, Ordering::SeqCst);
                        } else {
                            iteration_counter.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
            }
        });
    }

    async fn execute_protocol(
        &mut self,
        mut iteration_receiver: Receiver<IterationAdvance>,
        reset_sender: Sender<u32>,
        mut message_queue_receiver: Receiver<(NodeId, SimplexMessage, Signature)>,
        connections: &mut Vec<TcpStream>
    ) {
        loop {
            tokio::select! {
                Some(advance) = iteration_receiver.recv() => {
                    let iteration = self.iteration.load(Ordering::SeqCst);
                    match advance {
                        IterationAdvance::Timeout => {
                            let timeout = SimplexMessage::Timeout(Timeout { next_iter: iteration + 1 });
                            broadcast(&self.private_key, connections, timeout).await;
                        },
                        IterationAdvance::Notarization => {}
                    }

                    let leader = self.get_leader(iteration);
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
                        SimplexMessage::Propose(propose) => self.handle_propose(propose, sender, leader, connections).await,
                        SimplexMessage::Vote(vote) => self.handle_vote(vote.clone(), SimplexMessage::Vote(vote.clone()), sender, signature, reset_sender.clone(), connections).await,
                        SimplexMessage::Timeout(timeout) => self.handle_timeout(timeout, sender, reset_sender.clone()).await,
                        SimplexMessage::Finalize(finalize) => self.handle_finalize(finalize, sender),
                        SimplexMessage::View(view) => self.handle_view(view, connections, sender).await,
                        SimplexMessage::Request(request) => self.handle_request(request, connections, sender).await,
                        SimplexMessage::Reply(reply) => self.handle_reply(reply),
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

    async fn handle_propose(&mut self, propose: Propose, sender: u32, leader: u32, connections: &mut Vec<TcpStream>) {
        if !self.blocks.contains_key(&propose.content.iteration) && sender == leader {
            self.blocks.insert(propose.content.iteration, true);
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
        reset_sender: Sender<u32>,
        connections: &mut Vec<TcpStream>
    ) {
        let vote_signatures = self.votes.entry(Blockchain::hash(&vote.content)).or_insert_with(Vec::new);
        let is_first_vote = !vote_signatures.iter().any(|(_, node_id, _)| *node_id == sender);
        if is_first_vote {
            vote_signatures.push((message, sender, signature));
            if vote_signatures.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                let notarized_block = SimplexBlock { signatures: vote_signatures.to_vec(), ..vote.content };
                let clone_block = notarized_block.clone();
                self.blockchain.add_block(notarized_block);
                if vote.content.iteration == self.iteration.load(Ordering::SeqCst) {
                    match reset_sender.send(0).await {
                        Ok(_) => {},
                        Err(_) => return
                    };
                    let finalize = SimplexMessage::Finalize(Finalize { iter: vote.content.iteration});
                    broadcast(&self.private_key, connections, finalize).await;
                }
                let view = SimplexMessage::View(View { last_notarized_block: clone_block });
                broadcast(&self.private_key, connections, view).await;
            }
        }
    }

    async fn handle_timeout(&mut self, timeout: Timeout, sender: NodeId, reset_sender: Sender<u32>) {
        let timeouts = self.timeouts.entry(timeout.next_iter).or_insert_with(Vec::new);
        let is_first_timeout = !timeouts.iter().any(|node_id| *node_id == sender);
        if is_first_timeout {
            timeouts.push(sender);
            if timeouts.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                match reset_sender.send(timeout.next_iter).await {
                    Ok(_) => {},
                    Err(_) => return
                };
            }
        }
    }

    fn handle_finalize(&mut self, finalize: Finalize, sender: NodeId) {
        let finalizes = self.finalizes.entry(finalize.iter).or_insert_with(Vec::new);
        let is_first_finalize = !finalizes.iter().any(|node_id| *node_id == sender);
        if is_first_finalize {
            finalizes.push(sender);
            if finalizes.len() == self.environment.nodes.len() * 2 / 3 + 1 {
               self.blockchain.finalize(finalize.iter);
            }
        }
    }

    async fn handle_view(&self, view: View, connections: &mut Vec<TcpStream>, sender: NodeId) {
        // check view block signatures
        for (message, node_id, signature) in view.last_notarized_block.signatures.iter() {
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
        // check for missing blocks
        if self.blockchain.is_missing(view.last_notarized_block) {
            // ask for missing blocks to a process which notarized the block
            if let Some(sender_node) = self.environment.nodes.iter().find(|node| node.id == sender) {
                if let Some(stream) = connections.iter_mut().find(|stream| stream.peer_addr().unwrap().to_string() == format!("{}:{}", sender_node.host, sender_node.port)) {
                    unicast(&self.private_key, stream, SimplexMessage::Request(Request { last_notarized_block: self.blockchain.last_notarized() })).await
                }
            }
        }
    }

    async fn handle_request(&mut self, request: Request, connections: &mut Vec<TcpStream>, sender: NodeId) {
        let blocks = self.blockchain.get_missing(request.last_notarized_block);
        match blocks {
            Some(missing) => {
                // send missing blocks to the process which requested the blocks
                if let Some(sender_node) = self.environment.nodes.iter().find(|node| node.id == sender) {
                    if let Some(stream) = connections.iter_mut().find(|stream | stream.peer_addr().unwrap().to_string() == format!("{}:{}", sender_node.host, sender_node.port)) {
                        unicast(&self.private_key, stream, SimplexMessage::Reply(Reply { blocks: missing })).await
                    }
                }
            }
            None => {}
        }
    }

    fn handle_reply(&mut self, reply: Reply) {
        for block in reply.blocks {
            for (message, node_id, signature) in block.signatures.iter() {
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
                            Err(_) => return
                        }
                    }
                    None => return,
                }
            }
            self.blockchain.add_block(block);
        }
    }
}
