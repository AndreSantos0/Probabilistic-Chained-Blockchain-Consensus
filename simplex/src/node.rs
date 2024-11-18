use crate::blockchain::Blockchain;
use crate::message::{Propose, SimplexMessage, Timeout, Vote};
use ring::signature::Ed25519KeyPair;
use shared::connection::{accept_connections, broadcast, connect, NodeId, Signature};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use rand::prelude::StdRng;
use rand::SeedableRng;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{self, Duration};
use crate::block::SimplexBlock;

const SEED: u64 = 0xFF6DA736EA;
const INITIAL_ITERATION: u32 = 0;
const ITERATION_TIME: u64 = 3;
const MESSAGE_CHANNEL_SIZE: usize = 50;


pub struct SimplexNode {
    environment: Environment,
    iteration: Arc<AtomicU32>,
    blocks: HashMap<u32, bool>,
    votes: HashMap<u32, Vec<(NodeId, Signature)>>,
    timeouts: HashMap<Timeout, Vec<NodeId>>,
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
                self.start_iteration_timer(reset_rx, iter_tx);

                self.execute_protocol(iter_rx, reset_tx, rx, &mut connections).await;
            }
            Err(_) => { eprintln!("[Node {}] Failed to bind local port", self.environment.my_node.id) }
        };
    }


    fn start_iteration_timer(&self, mut reset_rx: Receiver<()>, iter_tx: Sender<IterationAdvance>) {
        let iteration_counter = self.iteration.clone();
        let mut timer = time::interval(Duration::from_secs(ITERATION_TIME));
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
                    _ = reset_rx.recv() => {
                        iteration_counter.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        });
    }

    async fn execute_protocol(
        &mut self,
        mut iteration_receiver: Receiver<IterationAdvance>,
        reset_sender: Sender<()>,
        mut message_queue_receiver: Receiver<(NodeId, SimplexMessage, Signature)>,
        connections: &mut Vec<TcpStream>
    ) {
        let mut rng = StdRng::seed_from_u64(SEED);
        loop {
            tokio::select! {
                Some(advance) = iteration_receiver.recv() => {
                    let iteration = self.iteration.load(Ordering::SeqCst);
                    match advance {
                        IterationAdvance::Timeout => {
                            let last_notarized = self.blockchain.last_notarized();
                            let timeout = SimplexMessage::Timeout(Timeout {
                                next_iter: iteration + 1,
                                last_notarized_iter: last_notarized.iteration,
                                last_notarized_block: last_notarized,
                            })
                            broadcast(&self.private_key, connections, timeout).await;
                        },
                        IterationAdvance::Notarization => {}
                    }

                    let leader = self.get_leader(iteration, &mut rng);
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
                    let leader = self.get_leader(iteration, &mut rng);
                    match message {
                        SimplexMessage::Propose(propose) => self.handle_propose(propose, sender, leader, connections).await,
                        SimplexMessage::Vote(vote) => self.handle_vote(vote, sender, signature).await,
                        SimplexMessage::Timeout(timeout) => self.handle_timeout(timeout, sender, reset_sender.clone()).await,
                        SimplexMessage::Finalize(finalize) => {}
                    }
                }
            }
        }
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

    async fn handle_vote(&mut self, vote: Vote, sender: NodeId, signature: Signature) {
        let vote_signatures = self.votes.entry(vote.content.iteration).or_insert_with(Vec::new);
        let is_first_vote = !vote_signatures.iter().any(|(node_id, _)| *node_id == sender);
        if is_first_vote {
            vote_signatures.push((sender, signature));
            if vote_signatures.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                let notarized_block = SimplexBlock { signatures: vote_signatures.to_vec(), ..vote.content.clone() };
                // add block
                // reset_sender.send(());
                // broadcast Finalize
            }
        }
    }

    async fn handle_timeout(&mut self, timeout: Timeout, sender: NodeId, reset_sender: Sender<()>) {
        let timeouts = self.timeouts.entry(timeout).or_insert_with(Vec::new);
        let is_first_timeout = !timeouts.iter().any(|node_id| *node_id == sender);
        if is_first_timeout {
            timeouts.push(sender);
            if timeouts.len() == self.environment.nodes.len() * 2 / 3 + 1 {
                match reset_sender.send(()).await {
                    Ok(_) => {},
                    Err(_) => return
                };
            }
        }
    }
}
