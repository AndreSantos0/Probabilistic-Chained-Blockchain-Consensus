use crate::block::{HashedSimplexBlock, NodeId, NotarizedBlock, SimplexBlock};
use crate::blockchain::Blockchain;
use crate::connection::{broadcast, connect};
use crate::message::SimplexMessage;
use ring::signature::Ed25519KeyPair;
use sha2::{Digest, Sha256};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{interval, Duration};

pub enum ProtocolMode {
    Practical,
    Probabilistic
}

pub trait Protocol {

    const INITIAL_ITERATION: u32;
    const ITERATION_TIME: u64;
    const MESSAGE_CHANNEL_SIZE: usize;
    const TIMEOUT_CHANNEL_SIZE: usize;
    const RESET_TIMER_CHANNEL_SIZE: usize;

    type Message: SimplexMessage;

    fn new(environment: Environment, public_keys: HashMap<u32, Vec<u8>>, private_key: Ed25519KeyPair) -> Self;
    fn get_environment(&self) -> &Environment;
    fn get_public_keys(&self) -> &HashMap<u32, Vec<u8>>;
    fn get_private_key(&self) -> &Ed25519KeyPair;
    fn get_iteration(&self) -> &Arc<AtomicU32>;
    fn get_is_timeout(&self) -> &Arc<AtomicBool>;
    fn get_blockchain(&mut self) -> &mut Blockchain;
    fn get_transaction_generator(&mut self) -> &mut TransactionGenerator;
    fn get_quorum_size(&self) -> usize;

    async fn start(&mut self)
    where
        Self: Sized, <Self as Protocol>::Message: 'static
    {
        let address = format!("{}:{}", self.get_environment().my_node.host, self.get_environment().my_node.port);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let (tx, rx) = mpsc::channel::<(NodeId, Self::Message)>(Self::MESSAGE_CHANNEL_SIZE);
                let (timeout_tx, timeout_rx) = mpsc::channel::<u32>(Self::TIMEOUT_CHANNEL_SIZE);
                let (reset_tx,reset_rx) = mpsc::channel::<()>(Self::RESET_TIMER_CHANNEL_SIZE);
                let sender = Arc::new(tx);
                let mut connections = connect(self.get_environment().my_node.id, &self.get_environment().nodes, self.get_public_keys(), sender, listener).await;
                self.start_iteration_timer(timeout_tx, reset_rx).await;
                self.execute_protocol(rx, timeout_rx, reset_tx, &mut connections).await;
            }
            Err(_) => { eprintln!("[Node {}] Failed to bind local port", self.get_environment().my_node.id) }
        };
    }

    async fn start_iteration_timer(&self, timeout_tx: Sender<u32>, mut reset_rx: Receiver<()>) {
        let iteration_counter = self.get_iteration().clone();
        let is_timeout = self.get_is_timeout().clone();
        let mut timer = interval(Duration::from_secs(Self::ITERATION_TIME));
        timer.tick().await;
        tokio::spawn(async move {
            loop {
                let iteration = iteration_counter.load(Ordering::SeqCst);
                tokio::select! {
                    _ = timer.tick() => {
                        if iteration == iteration_counter.load(Ordering::SeqCst) {
                            is_timeout.store(true, Ordering::SeqCst);
                            let _ = timeout_tx.send(iteration + 1).await;
                            reset_rx.recv().await;
                            timer = interval(Duration::from_secs(Self::ITERATION_TIME));
                            timer.tick().await;
                        }
                    }
                    _ = reset_rx.recv() => {
                        timer = interval(Duration::from_secs(Self::ITERATION_TIME));
                        timer.tick().await;
                    }
                }
            }
        });
    }

    fn get_leader(n_nodes: usize, iteration: u32) -> u32 {
        let mut hasher = Sha256::new();
        hasher.update(&iteration.to_le_bytes());
        let hash = hasher.finalize();
        let hash_u64 = u64::from_le_bytes(hash[0..8].try_into().expect("Invalid slice length"));
        (hash_u64 % n_nodes as u64) as u32
    }

    async fn handle_iteration_advance(&mut self, connections: &mut Vec<Option<TcpStream>>) {
        loop {
            let iteration = self.get_iteration().load(Ordering::SeqCst);
            let leader = Self::get_leader(self.get_environment().nodes.len(), iteration);
            self.get_is_timeout().store(false, Ordering::SeqCst);
            self.clear_timeouts(iteration);
            println!("----------------------");
            println!("----------------------");
            println!("Leader is node {} | Iteration: {}", leader, iteration);
            self.get_blockchain().print();

            let my_node_id = self.get_environment().my_node.id;
            if leader == my_node_id {
                let transactions = self.get_transaction_generator().generate(my_node_id);
                let block = self.get_blockchain().get_next_block(iteration, transactions);
                //println!("Proposed {}", block.length);
                let message = Self::create_proposal(block);
                self.send(connections, message, None).await;
            }

            if let Some(propose) = self.get_proposal_block(iteration) {
                let proposed_block = propose.clone();
                if !self.get_is_timeout().load(Ordering::SeqCst) && self.get_blockchain().is_extendable(&proposed_block) {
                    let block = HashedSimplexBlock::from(&proposed_block);
                    let vote = self.create_vote(iteration, block);
                    let sample = vote.get_sample();
                    self.send(connections, vote, sample).await;
                    //println!("Voted for {}", proposed_block.length);
                }
            }

            if self.get_timeouts(iteration + 1) >= self.get_quorum_size() {
                self.get_iteration().store(iteration + 1, Ordering::SeqCst);
                continue;
            }

            match self.get_blockchain().check_for_possible_notarization(iteration) {
                None => break,
                Some(notarized) => {
                    self.get_blockchain().notarize(notarized.block.clone(), notarized.transactions.clone(), notarized.signatures.clone()).await;
                    self.get_iteration().fetch_add(1, Ordering::SeqCst);
                    if !self.get_is_timeout().load(Ordering::SeqCst) {
                        let finalize = self.create_finalize(iteration);
                        let sample = finalize.get_sample();
                        self.send(connections, finalize, sample).await;
                    }
                    self.post_notarization(notarized, connections).await;
                }
            }
        }
    }

    async fn execute_protocol(
        &mut self,
        mut message_queue_receiver: Receiver<(u32, Self::Message)>,
        mut timeout_rx: Receiver<u32>,
        reset_tx: Sender<()>,
        connections: &mut Vec<Option<TcpStream>>,
    )
    where
        Self: Sized,
    {
        self.handle_iteration_advance(connections).await;
        loop {
            tokio::select! {
                Some(next_iter) = timeout_rx.recv() => {
                    if next_iter - 1 == self.get_iteration().load(Ordering::SeqCst) {
                        let timeout = Self::create_timeout(next_iter);
                        broadcast(self.get_private_key(), connections, timeout).await;
                    }
                }

                Some((sender, message)) = message_queue_receiver.recv() => {
                    self.handle_message(sender, message, reset_tx.clone(), connections).await;
                }
            }
        }
    }

    async fn send(&self, connections: &mut Vec<Option<TcpStream>>, message: Self::Message, recipients: Option<Vec<u32>>);
    fn create_proposal(block: SimplexBlock) -> Self::Message;
    fn create_timeout(next_iteration: u32) -> Self::Message;
    fn create_vote(&self, iteration: u32, block: HashedSimplexBlock) -> Self::Message;
    fn create_finalize(&self, iteration: u32) -> Self::Message;
    fn get_proposal_block(&self, iteration: u32) -> Option<&SimplexBlock>;
    fn get_timeouts(&self, iteration: u32) -> usize;
    fn clear_timeouts(&mut self, iteration: u32);
    async fn handle_message(&mut self, sender: u32, message: Self::Message, reset_tx: Sender<()>, connections: &mut Vec<Option<TcpStream>>);
    async fn post_notarization(&self, notarized: NotarizedBlock, connections: &mut Vec<Option<TcpStream>>);
}
