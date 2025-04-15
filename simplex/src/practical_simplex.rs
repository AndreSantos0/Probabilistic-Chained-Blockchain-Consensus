use crate::block::{HashedSimplexBlock, NotarizedBlock, SimplexBlock, ViewBlock, VoteSignature};
use crate::blockchain::Blockchain;
use crate::connection::{broadcast, notify, unicast};
use crate::message::{Finalize, PracticalSimplexMessage, Propose, Reply, Request, Timeout, View, Vote};
use crate::protocol::Protocol;
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519};
use serde_json::to_string;
use sha2::{Digest, Sha256};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;

pub struct PracticalSimplex {
    environment: Environment,
    quorum_size: usize,
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

impl Protocol for PracticalSimplex {
    const INITIAL_ITERATION: u32 = 1;
    const ITERATION_TIME: u64 = 10;
    const MESSAGE_CHANNEL_SIZE: usize = 100;
    const TIMEOUT_CHANNEL_SIZE: usize = 1;
    const RESET_TIMER_CHANNEL_SIZE: usize = 1;
    type Message = PracticalSimplexMessage;

    fn new(environment: Environment, public_keys: HashMap<u32, Vec<u8>>, private_key: Ed25519KeyPair) -> Self {
        let my_node_id = environment.my_node.id;
        let n = environment.nodes.len();
        PracticalSimplex {
            environment,
            quorum_size: n * 2 / 3 + 1,
            iteration: Arc::new(AtomicU32::new(Self:: INITIAL_ITERATION)),
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

    fn get_environment(&self) -> &Environment {
        &self.environment
    }

    fn get_public_keys(&self) -> &HashMap<u32, Vec<u8>> {
        &self.public_keys
    }

    fn get_private_key(&self) -> &Ed25519KeyPair {
        &self.private_key
    }

    fn get_iteration(&self) -> &Arc<AtomicU32> {
        &self.iteration
    }

    fn get_is_timeout(&self) -> &Arc<AtomicBool> {
        &self.is_timeout
    }

    fn get_blockchain(&mut self) -> &mut Blockchain {
        &mut self.blockchain
    }

    fn get_transaction_generator(&mut self) -> &mut TransactionGenerator {
        &mut self.transaction_generator
    }

    fn get_quorum_size(&self) -> usize {
        self.quorum_size
    }

    async fn send(&self, connections: &mut Vec<Option<TcpStream>>, message: Self::Message, _recipients: Option<Vec<u32>>) {
        broadcast(self.get_private_key(), connections, message).await;
    }

    fn create_proposal(block: SimplexBlock) -> Self::Message {
        PracticalSimplexMessage::Propose(Propose { content: block })
    }

    fn create_timeout(next_iteration: u32) -> Self::Message {
        PracticalSimplexMessage::Timeout(Timeout { next_iter: next_iteration })
    }

    fn create_vote(&self, iteration: u32, block: HashedSimplexBlock) -> Self::Message {
        let serialized_message = to_string(&block).unwrap();
        let serialized_bytes = serialized_message.as_bytes();
        PracticalSimplexMessage::Vote(Vote { iteration, header: block, signature: Vec::from(self.private_key.sign(serialized_bytes).as_ref()) })
    }

    fn create_finalize(&self, iteration: u32) -> Self::Message {
        PracticalSimplexMessage::Finalize(Finalize { iter: iteration })
    }

    fn get_proposal_block(&self, iteration: u32) -> Option<&SimplexBlock> {
        self.proposes.get(&iteration).map(|propose| &propose.content)
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

    async fn handle_message(&mut self, sender: u32, message: Self::Message, reset_tx: Sender<()>, connections: &mut Vec<Option<TcpStream>>) {
        match message {
            PracticalSimplexMessage::Propose(propose) => self.handle_propose(propose, sender, connections).await,
            PracticalSimplexMessage::Vote(vote) => self.handle_vote(vote.clone(), sender, reset_tx.clone(), connections).await,
            PracticalSimplexMessage::Timeout(timeout) => self.handle_timeout(timeout, sender, reset_tx.clone(), connections).await,
            PracticalSimplexMessage::Finalize(finalize) => self.handle_finalize(finalize, sender, connections).await,
            PracticalSimplexMessage::View(view) => self.handle_view(view, connections, sender).await,
            PracticalSimplexMessage::Request(request) => self.handle_request(request, connections, sender).await,
            PracticalSimplexMessage::Reply(reply) => self.handle_reply(reply, reset_tx.clone(), connections).await,
        }
    }

    async fn post_notarization(&self, notarized: NotarizedBlock, connections: &mut Vec<Option<TcpStream>>) {
        let view = PracticalSimplexMessage::View(View { last_notarized: ViewBlock { block: notarized.block, signatures: notarized.signatures }});
        notify(&self.private_key, connections, view, self.environment.my_node.id).await;
    }
}

impl PracticalSimplex {

    async fn handle_propose(&mut self, propose: Propose, sender: u32, connections: &mut Vec<Option<TcpStream>>) {
        println!("Received propose {}", propose.content.length);
        let leader = Self::get_leader(self.environment.nodes.len(), propose.content.iteration);
        if !self.proposes.contains_key(&propose.content.iteration) && sender == leader {
            self.proposes.insert(propose.content.iteration, propose.clone());
            let iteration = self.iteration.load(Ordering::SeqCst);
            if iteration == propose.content.iteration && !self.is_timeout.load(Ordering::SeqCst) && self.blockchain.is_extendable(&propose.content) {
                let block = HashedSimplexBlock::from(&propose.content);
                let vote = self.create_vote(iteration, block);
                self.send(connections, vote, None).await;
                println!("Voted for {}", propose.content.length);
            }
        }
    }

    async fn handle_vote(
        &mut self,
        vote: Vote,
        sender: u32,
        reset_tx: Sender<()>,
        connections: &mut Vec<Option<TcpStream>>,
    ) {
        println!("Received vote {}", vote.iteration);
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
                        if vote.iteration == iteration {
                            self.blockchain.notarize(
                                HashedSimplexBlock::from(&block.content),
                                block.content.transactions.clone(),
                                vote_signatures.to_vec()
                            ).await;
                            self.iteration.fetch_add(1, Ordering::SeqCst);
                            if !is_timeout {
                                let finalize = PracticalSimplexMessage::Finalize(Finalize { iter: iteration });
                                broadcast(&self.private_key, connections, finalize).await;
                            }
                            let view = PracticalSimplexMessage::View(View { last_notarized: ViewBlock { block: HashedSimplexBlock::from(&block.content), signatures: vote_signatures.to_vec() }});
                            notify(&self.private_key, connections, view, self.environment.my_node.id).await;
                            self.handle_iteration_advance(connections).await;
                            let _ = reset_tx.send(()).await;
                        } else if vote.iteration > iteration {
                            self.blockchain.add_to_be_notarized(
                                HashedSimplexBlock::from(&block.content),
                                block.content.transactions.clone(),
                                vote_signatures.to_vec()
                            );
                            self.request(sender, connections).await;
                        }
                    }
                }
            }
        }
    }

    async fn handle_timeout(&mut self, timeout: Timeout, sender: u32, reset_tx: Sender<()>, connections: &mut Vec<Option<TcpStream>>) {
        println!("Received timeout {}", timeout.next_iter);
        let timeouts = self.timeouts.entry(timeout.next_iter).or_insert_with(Vec::new);
        let is_first_timeout = !timeouts.iter().any(|node_id| *node_id == sender);
        if is_first_timeout {
            timeouts.push(sender);
            println!("{} matching timeouts for iter {}", timeouts.len(), timeout.next_iter);
            //if let Some(notarized) = self.blockchain.get_block(timeout.next_iter - 1) {
            //    unicast(&self.private_key, connections, PracticalSimplexMessage::Reply(Reply { blocks: vec![notarized.clone()] }), sender).await;
            //}
            if timeouts.len() == self.quorum_size && timeout.next_iter == self.iteration.load(Ordering::SeqCst) + 1 {
                self.iteration.store(timeout.next_iter, Ordering::SeqCst);
                self.handle_iteration_advance(connections).await;
                let _ = reset_tx.send(()).await;
            }
        }
    }

    async fn handle_finalize(&mut self, finalize: Finalize, sender: u32, connections: &mut Vec<Option<TcpStream>>) {
        println!("Received finalize {}", finalize.iter);
        let finalizes = self.finalizes.entry(finalize.iter).or_insert_with(Vec::new);
        let is_first_finalize = !finalizes.iter().any(|node_id| *node_id == sender);
        if is_first_finalize {
            finalizes.push(sender);
            if finalizes.len() == self.quorum_size {
                if let Some(_) = self.blockchain.get_block(finalize.iter) {
                    if !self.environment.test_flag {
                        self.blockchain.finalize(finalize.iter).await;
                    }
                    self.proposes.retain(|iteration, _| *iteration > finalize.iter);
                    self.votes.retain(|_, signatures| signatures.len() < self.environment.nodes.len() * 2 / 3 + 1);
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

    async fn handle_view(
        &mut self,
        view: View,
        connections: &mut Vec<Option<TcpStream>>,
        sender: u32,
    ) {
        println!("Received view {}", view.last_notarized.block.length);
        if self.blockchain.is_missing(view.last_notarized.block.length, view.last_notarized.block.iteration) && view.last_notarized.signatures.len() >= self.quorum_size {
            for vote_signature in view.last_notarized.signatures.iter() {
                match self.public_keys.get(&vote_signature.node) {
                    None => return,
                    Some(key) => {
                        let public_key = UnparsedPublicKey::new(&ED25519, key);
                        let serialized_message = match to_string(&view.last_notarized.block) {
                            Ok(json) => json,
                            Err(e) => {
                                eprintln!("Failed to serialize message: {}", e);
                                return;
                            }
                        };
                        let bytes = serialized_message.as_bytes();
                        match public_key.verify(bytes, vote_signature.signature.as_ref()) {
                            Ok(_) => { }
                            Err(_) => {
                                println!("View signature verification failed");
                                return
                            }
                        }
                    }
                }
            }
            self.request(sender, connections).await;
        }
    }

    async fn handle_request(&mut self, request: Request, connections: &mut Vec<Option<TcpStream>>, sender: u32) {
        println!("Request received");
        let missing = self.blockchain.get_missing(request.last_notarized_length).await;
        if missing.is_empty() {
            return
        }
        if let Some(sender_node) = self.environment.nodes.iter().find(|node| node.id == sender) {
            unicast(&self.private_key, connections, PracticalSimplexMessage::Reply(Reply { blocks: missing }), sender_node.id).await;
            println!("Reply send");
        }
    }

    async fn handle_reply(
        &mut self,
        reply: Reply,
        reset_tx: Sender<()>,
        connections: &mut Vec<Option<TcpStream>>,
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
                if let Some(_) = self.blockchain.find_parent_block(&notarized.block.hash) {
                    self.blockchain.notarize(notarized.block.clone(), notarized.transactions.clone(), notarized.signatures.clone()).await;
                    if notarized.block.iteration == iteration {
                        is_reset = true;
                        self.iteration.swap(notarized.block.iteration + 1, Ordering::SeqCst);
                        if !self.is_timeout.load(Ordering::SeqCst) {
                            let finalize = PracticalSimplexMessage::Finalize(Finalize { iter: iteration });
                            broadcast(&self.private_key, connections, finalize).await;
                        }
                        let view = PracticalSimplexMessage::View(View { last_notarized: ViewBlock { block: notarized.block, signatures: notarized.signatures } });
                        notify(&self.private_key, connections, view, self.environment.my_node.id).await;
                        self.handle_iteration_advance(connections).await;
                    }
                }
            }
        }
        if is_reset {
            let _ = reset_tx.send(()).await;
        }
    }

    async fn request(&self, sender: u32, connections: &mut Vec<Option<TcpStream>>) {
        if let Some(sender_node) = self.environment.nodes.iter().find(|node| node.id == sender) {
            unicast(&self.private_key, connections, PracticalSimplexMessage::Request(Request { last_notarized_length: self.blockchain.last_notarized().block.length }), sender_node.id).await;
            println!("Request send");
        }
    }
}
