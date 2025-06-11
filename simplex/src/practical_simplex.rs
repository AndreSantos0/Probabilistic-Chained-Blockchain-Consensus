use crate::block::{NodeId, NotarizedBlock, SimplexBlock, SimplexBlockHeader, VoteSignature};
use crate::blockchain::Blockchain;
use crate::connection::{broadcast, generate_nonce, unicast, MESSAGE_BYTES_LENGTH, NONCE_BYTES_LENGTH, SIGNATURE_BYTES_LENGTH};
use crate::message::{Dispatch, Finalize, PracticalSimplexMessage, Propose, Reply, Request, SimplexMessage, Timeout, View, Vote};
use crate::protocol::Protocol;
use sha2::{Digest, Sha256};
use shared::domain::environment::Environment;
use shared::transaction_generator::TransactionGenerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use bincode::{deserialize, serialize};
use ed25519_dalek::{verify_batch, Keypair, PublicKey, Signature, Signer, Verifier};
use log::{error, info, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use shared::initializer::get_private_key;


pub struct PracticalSimplex {
    environment: Environment,
    quorum_size: usize,
    iteration: Arc<AtomicU32>,
    is_timeout: Arc<AtomicBool>,
    blocks_finalized: usize,
    proposes: HashMap<u32, Propose>,
    votes: HashMap<SimplexBlockHeader, Vec<VoteSignature>>,
    timeouts: HashMap<u32, Vec<u32>>,
    finalizes: HashMap<u32, Vec<u32>>,
    blockchain: Blockchain,
    transaction_generator: TransactionGenerator,
    public_keys: HashMap<u32, PublicKey>,
    private_key: Keypair,
}

impl Protocol for PracticalSimplex {

    type Message = PracticalSimplexMessage;

    fn new(environment: Environment, public_keys: HashMap<u32, PublicKey>, private_key: Keypair) -> Self {
        let my_node_id = environment.my_node.id;
        let n = environment.nodes.len();
        let transaction_size = environment.transaction_size;
        let n_transactions = environment.n_transactions;
        PracticalSimplex {
            environment,
            quorum_size: n * 2 / 3 + 1,
            iteration: Arc::new(AtomicU32::new(Self::INITIAL_ITERATION)),
            is_timeout: Arc::new(AtomicBool::new(false)),
            blocks_finalized: 0,
            proposes: HashMap::new(),
            votes: HashMap::new(),
            timeouts: HashMap::new(),
            finalizes: HashMap::new(),
            blockchain: Blockchain::new(my_node_id),
            transaction_generator: TransactionGenerator::new(transaction_size, n_transactions),
            public_keys,
            private_key
        }
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

    async fn connect(&self, message_queue_sender: Sender<(NodeId, PracticalSimplexMessage)>, listener: TcpListener) -> Vec<Option<TcpStream>> {
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
                    let quorum_size = self.quorum_size;
                    tokio::spawn(async move {
                        Self::handle_connection(enable_crypto, stream, sender, &public_keys, quorum_size, my_node_id, claimed_id).await;
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
        message_queue_sender: &Sender<(NodeId, PracticalSimplexMessage)>,
        mut dispatcher_queue_receiver: Receiver<Dispatch>,
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

    async fn handle_iteration_advance(&mut self, dispatcher_queue_sender: &Sender<Dispatch>, finalize_sender: &Sender<Vec<NotarizedBlock>>) {
        loop {
            let iteration = self.iteration.load(Ordering::Acquire);
            let leader = Self::get_leader(self.environment.nodes.len(), iteration);
            self.is_timeout.store(false, Ordering::Release);
            self.clear_timeouts(iteration);
            info!("----------------------");
            info!("----------------------");
            info!("Leader is node {} | Iteration: {}", leader, iteration);
            //self.blockchain().print();

            let my_node_id = self.environment.my_node.id;
            if leader == my_node_id {
                let transactions = self.transaction_generator.generate();
                let block = self.blockchain.get_next_block(iteration, transactions);
                info!("Proposed {}", block.length);
                let propose = self.create_proposal(block);
                let _ = dispatcher_queue_sender.send(propose).await;
            }

            if let Some(propose) = self.get_proposal_block(iteration) {
                if !self.is_timeout.load(Ordering::Acquire) && self.blockchain.is_extendable(propose) {
                    let block = SimplexBlockHeader::from(propose);
                    let vote = Self::create_vote(iteration, block);
                    let _ = dispatcher_queue_sender.send(vote).await;
                }
            }

            if self.get_timeouts(iteration + 1) >= self.quorum_size {
                self.iteration.store(iteration + 1, Ordering::Release);
                continue;
            }

            match self.blockchain.check_for_possible_notarization(iteration) {
                None => break,
                Some(notarized) => {
                    let propose = self.proposes.remove(&iteration).unwrap();
                    self.blockchain.notarize(notarized.block.clone(), propose.content.transactions, notarized.signatures.clone(), finalize_sender).await;
                    self.iteration.fetch_add(1, Ordering::AcqRel);
                    if !self.is_timeout.load(Ordering::Acquire) {
                        let finalize = Self::create_finalize(iteration);
                        let _ = dispatcher_queue_sender.send(finalize).await;
                    }
                    let _ = dispatcher_queue_sender.send(Dispatch::View(notarized.block, notarized.signatures)).await;
                }
            }
        }
    }

    async fn handle_message(&mut self, sender: u32, message: PracticalSimplexMessage, dispatcher_queue_sender: &Sender<Dispatch>, reset_timer_sender: &Sender<()>, finalize_sender: &Sender<Vec<NotarizedBlock>>) {
        match message {
            PracticalSimplexMessage::Propose(propose) => self.handle_propose(propose, sender, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await,
            PracticalSimplexMessage::Vote(vote) => self.handle_vote(vote, sender, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await,
            PracticalSimplexMessage::Timeout(timeout) => self.handle_timeout(timeout, sender, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await,
            PracticalSimplexMessage::Finalize(finalize) => self.handle_finalize(finalize, sender, dispatcher_queue_sender, finalize_sender).await,
            PracticalSimplexMessage::View(view) => self.handle_view(view, sender, dispatcher_queue_sender).await,
            PracticalSimplexMessage::Request(request) => self.handle_request(request, sender, dispatcher_queue_sender).await,
            PracticalSimplexMessage::Reply(reply) => self.handle_reply(reply, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await,
        }
    }

    fn create_proposal(&self, block: SimplexBlock) -> Dispatch {
        Dispatch::Propose(block)
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
}

impl PracticalSimplex {

    async fn dispatch_message(
        message_queue_sender: &Sender<(NodeId, PracticalSimplexMessage)>,
        content: Dispatch,
        private_key: &Keypair,
        enable_crypto: bool,
        my_node_id: u32,
        connections: &mut Vec<Option<TcpStream>>
    ) {
        match content {
            Dispatch::Propose(block) => {
                let propose = PracticalSimplexMessage::Propose(Propose { content: block });
                broadcast(private_key, connections, &propose, enable_crypto).await;
                let _ = message_queue_sender.send((my_node_id, propose)).await;
            }
            Dispatch::Vote(iteration, header) => {
                let signature = if !enable_crypto {
                    Vec::new()
                } else {
                    let serialized_message = serialize(&header).expect("Failed to serialize block header during vote");
                    private_key.sign(&serialized_message).as_ref().to_vec()
                };

                let vote = PracticalSimplexMessage::Vote(Vote {
                    iteration,
                    header,
                    signature
                });
                broadcast(private_key, connections, &vote, enable_crypto).await;
                let _ = message_queue_sender.send((my_node_id, vote)).await;
            }
            Dispatch::Timeout(next_iter) => {
                let timeout = PracticalSimplexMessage::Timeout(Timeout { next_iter });
                broadcast(private_key, connections, &timeout, enable_crypto).await;
                let _ = message_queue_sender.send((my_node_id, timeout)).await;
            }
            Dispatch::Finalize(iter) => {
                let finalize = PracticalSimplexMessage::Finalize(Finalize { iter });
                broadcast(private_key, connections, &finalize, enable_crypto).await;
                let _ = message_queue_sender.send((my_node_id, finalize)).await;
            }
            Dispatch::View(header, cert) => {
                let view = PracticalSimplexMessage::View(View { last_notarized_block_header: header, last_notarized_block_cert: cert });
                broadcast(private_key, connections, &view, enable_crypto).await;
            }
            Dispatch::Request(last_notarized_length, sender) => {
                let request = PracticalSimplexMessage::Request(Request { last_notarized_length });
                unicast(private_key, connections, &request, sender, my_node_id, enable_crypto).await;
            }
            Dispatch::Reply(blocks, sender) => {
                let reply = PracticalSimplexMessage::Reply(Reply { blocks });
                unicast(private_key, connections, &reply, sender, my_node_id, enable_crypto).await;
            }
            _ => {}
        }
    }

    async fn handle_connection(
        enable_crypto: bool,
        mut stream: TcpStream,
        message_queue_sender: Sender<(NodeId, PracticalSimplexMessage)>,
        public_keys: &HashMap<u32, PublicKey>,
        quorum_size: usize,
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

            let message: PracticalSimplexMessage = match deserialize(&buffer) {
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

                match &message {
                    PracticalSimplexMessage::Reply(reply) => {
                        if reply.blocks.is_empty() { continue }
                        for notarized in &reply.blocks {
                            let transactions_data = match serialize(&notarized.transactions) {
                                Ok(data) => data,
                                Err(_) => {
                                    error!("[Node {}] Failed to serialize block transactions during reply", my_node_id);
                                    continue;
                                }
                            };
                            let hashed_transactions = Sha256::digest(&transactions_data).to_vec();
                            if hashed_transactions != notarized.block.transactions { continue }
                            if notarized.signatures.len() >= quorum_size {
                                let serialized_message = match serialize(&notarized.block) {
                                    Ok(msg) => msg,
                                    Err(_) => {
                                        error!("[Node {}] Failed to serialize vote signature header", my_node_id);
                                        continue;
                                    }
                                };

                                let messages: Vec<&[u8]> = (0..notarized.signatures.len()).map(|_| serialized_message.as_slice()).collect();
                                let signatures: Vec<Signature>  = notarized.signatures.iter().map(
                                    |vote_signature| Signature::from_bytes(vote_signature.signature.as_ref()).unwrap()
                                ).collect();
                                let mut keys = Vec::with_capacity(notarized.signatures.len());
                                for vote_signature in &notarized.signatures {
                                    match public_keys.get(&vote_signature.node) {
                                        Some(key) => keys.push(*key),
                                        None => continue,
                                    }
                                }

                                if !verify_batch(&messages[..], &signatures[..], &keys[..]).is_ok() {
                                    continue;
                                }
                            } else { continue }
                        }
                    }
                    _ => {}
                }
            }
            let _ = message_queue_sender.send((node_id, message)).await;
        }
    }

    async fn handle_propose(&mut self, propose: Propose, sender: u32, dispatcher_queue_sender: &Sender<Dispatch>, reset_timer_sender: &Sender<()>, finalize_sender: &Sender<Vec<NotarizedBlock>>) {
        info!("Received propose {}", propose.content.length);
        let leader = Self::get_leader(self.environment.nodes.len(), propose.content.iteration);
        if !self.proposes.contains_key(&propose.content.iteration) && sender == leader {
            let iteration = self.iteration.load(Ordering::Acquire);
            if iteration == propose.content.iteration && !self.is_timeout.load(Ordering::Acquire) && self.blockchain.is_extendable(&propose.content) {
                let block = SimplexBlockHeader::from(&propose.content);
                let vote = Self::create_vote(iteration, block);
                let _ = dispatcher_queue_sender.send(vote).await;
            }
            if iteration == propose.content.iteration {
                let block = SimplexBlockHeader::from(&propose.content);
                let votes = self.votes.get(&block);
                if let Some(vote_signatures) = votes {
                    if vote_signatures.len() >= self.quorum_size {
                        let is_timeout = self.is_timeout.load(Ordering::Acquire);
                        self.blockchain.notarize(
                            block.clone(),
                            propose.content.transactions,
                            vote_signatures.to_vec(),
                            finalize_sender
                        ).await;
                        self.iteration.fetch_add(1, Ordering::AcqRel);
                        if !is_timeout {
                            let finalize = Self::create_finalize(iteration);
                            let _ = dispatcher_queue_sender.send(finalize).await;
                        }
                        let view = Dispatch::View(block, vote_signatures.to_vec());
                        let _ = dispatcher_queue_sender.send(view).await;
                        self.handle_iteration_advance(dispatcher_queue_sender, finalize_sender).await;
                        let _ = reset_timer_sender.send(()).await;
                        return;
                    }
                }
            }
            self.proposes.insert(propose.content.iteration, propose);
        }
    }

    async fn handle_vote(
        &mut self,
        vote: Vote,
        sender: u32,
        dispatcher_queue_sender: &Sender<Dispatch>,
        reset_timer_sender: &Sender<()>,
        finalize_sender: &Sender<Vec<NotarizedBlock>>,
    ) {
        info!("Received vote {} from {}", vote.iteration, sender);
        let vote_signatures = self.votes.entry(vote.header).or_insert_with(Vec::new);
        let is_first_vote = !vote_signatures.iter().any(|vote_signature| vote_signature.node == sender);
        if is_first_vote {
            vote_signatures.push(VoteSignature { signature: vote.signature, node: sender });
            info!("{} signatures", vote_signatures.len());
            let iteration = self.iteration.load(Ordering::Acquire);
            if vote_signatures.len() == self.quorum_size {
                match self.proposes.get(&vote.iteration) {
                    None => return,
                    Some(block) => {
                        let is_timeout = self.is_timeout.load(Ordering::Acquire);
                        if vote.iteration == iteration {
                            let block = self.proposes.remove(&vote.iteration).unwrap();
                            let header = SimplexBlockHeader::from(&block.content);
                            self.blockchain.notarize(
                                header.clone(),
                                block.content.transactions,
                                vote_signatures.to_vec(),
                                finalize_sender
                            ).await;
                            self.iteration.fetch_add(1, Ordering::AcqRel);
                            if !is_timeout {
                                let finalize = Self::create_finalize(iteration);
                                let _ = dispatcher_queue_sender.send(finalize).await;
                            }
                            let view = Dispatch::View(header, vote_signatures.to_vec());
                            let _ = dispatcher_queue_sender.send(view).await;
                            self.handle_iteration_advance(dispatcher_queue_sender, finalize_sender).await;
                            let _ = reset_timer_sender.send(()).await;
                        } else if vote.iteration > iteration {
                            self.blockchain.add_to_be_notarized(
                                SimplexBlockHeader::from(&block.content),
                                vote_signatures.to_vec()
                            );
                            self.request(sender, dispatcher_queue_sender).await;
                        }
                    }
                }
            }
        }
    }

    async fn handle_timeout(&mut self, timeout: Timeout, sender: u32, dispatcher_queue_sender: &Sender<Dispatch>, reset_timer_sender: &Sender<()>, finalize_sender: &Sender<Vec<NotarizedBlock>>) {
        info!("Received timeout {}", timeout.next_iter);
        let timeouts = self.timeouts.entry(timeout.next_iter).or_insert_with(Vec::new);
        let is_first_timeout = !timeouts.iter().any(|node_id| *node_id == sender);
        if is_first_timeout {
            timeouts.push(sender);
            info!("{} matching timeouts for iter {}", timeouts.len(), timeout.next_iter);
            if timeouts.len() == self.quorum_size && timeout.next_iter == self.iteration.load(Ordering::Acquire) + 1 {
                self.iteration.store(timeout.next_iter, Ordering::Release);
                self.handle_iteration_advance(dispatcher_queue_sender, finalize_sender).await;
                let _ = reset_timer_sender.send(()).await;
            }
        }
    }

    async fn handle_finalize(&mut self, finalize: Finalize, sender: u32, dispatcher_queue_sender: &Sender<Dispatch>, finalize_sender: &Sender<Vec<NotarizedBlock>>) {
        info!("Received finalize {} from {}", finalize.iter, sender);
        let finalizes = self.finalizes.entry(finalize.iter).or_insert_with(Vec::new);
        let is_first_finalize = !finalizes.iter().any(|node_id| *node_id == sender);
        if is_first_finalize {
            finalizes.push(sender);
            if finalizes.len() == self.quorum_size {
                if let Some(_) = self.blockchain.get_block(finalize.iter) {
                    let blocks_finalized = self.blockchain.finalize(finalize.iter, finalize_sender).await;
                    self.blocks_finalized += blocks_finalized;
                    self.proposes.retain(|iteration, _| *iteration > finalize.iter);
                    self.votes.retain(|_, signatures| signatures.len() < self.environment.nodes.len() * 2 / 3 + 1);
                    self.finalizes.retain(|iteration, _| *iteration > finalize.iter);
                } else {
                    self.blockchain.add_to_be_finalized(finalize.iter);
                    self.request(sender, dispatcher_queue_sender).await;
                }
            }
        }
    }

    async fn handle_view(
        &mut self,
        view: View,
        sender: u32,
        dispatcher_queue_sender: &Sender<Dispatch>,
    ) {
        info!("Received view {}", view.last_notarized_block_header.length);
        if self.blockchain.is_missing(view.last_notarized_block_header.length, view.last_notarized_block_header.iteration) && view.last_notarized_block_cert.len() >= self.quorum_size {
            let serialized_message = match serialize(&view.last_notarized_block_header) {
                Ok(msg) => msg,
                Err(_) => {
                    error!("[Node {}] Failed to deserialize vote signature header", self.environment.my_node.id);
                    return;
                }
            };

            let messages: Vec<&[u8]> = (0..view.last_notarized_block_cert.len()).map(|_| serialized_message.as_slice()).collect();
            let signatures: Vec<Signature>  = view.last_notarized_block_cert.iter().map(
                |vote_signature| Signature::from_bytes(vote_signature.signature.as_ref()).unwrap()
            ).collect();
            let mut keys = Vec::with_capacity(view.last_notarized_block_cert.len());
            for vote_signature in &view.last_notarized_block_cert {
                match self.public_keys.get(&vote_signature.node) {
                    Some(key) => keys.push(*key),
                    None => return,
                }
            }

            if !verify_batch(&messages[..], &signatures[..], &keys[..]).is_ok() {
                return;
            }
            self.request(sender, dispatcher_queue_sender).await;
        }
    }

    async fn handle_request(&self, request: Request, sender: u32, dispatcher_queue_sender: &Sender<Dispatch>) {
        info!("Request received");
        self.blockchain.get_missing(request.last_notarized_length, sender, dispatcher_queue_sender).await;
    }

    async fn handle_reply(
        &mut self,
        reply: Reply,
        dispatcher_queue_sender: &Sender<Dispatch>,
        reset_timer_sender: &Sender<()>,
        finalize_sender: &Sender<Vec<NotarizedBlock>>
    ) {
        info!("Received Reply {:?}", reply.blocks);
        let mut is_reset = false;
        for notarized in reply.blocks {
            if self.blockchain.is_missing(notarized.block.length, notarized.block.iteration) {
                let iteration = self.iteration.load(Ordering::Acquire);
                if let Some(_) = self.blockchain.find_parent_block(&notarized.block.hash) {
                    self.blockchain.notarize(notarized.block.clone(), notarized.transactions, notarized.signatures.clone(), finalize_sender).await;
                    if notarized.block.iteration == iteration {
                        is_reset = true;
                        self.iteration.swap(notarized.block.iteration + 1, Ordering::AcqRel);
                        if !self.is_timeout.load(Ordering::Acquire) {
                            let finalize = Self::create_finalize(iteration);
                            let _ = dispatcher_queue_sender.send(finalize).await;
                        }
                        let view = Dispatch::View(notarized.block, notarized.signatures);
                        let _ = dispatcher_queue_sender.send(view).await;
                        self.handle_iteration_advance(dispatcher_queue_sender, finalize_sender).await;
                    }
                }
            } else { break }
        }
        if is_reset {
            let _ = reset_timer_sender.send(()).await;
        }
    }

    async fn request(&self, sender: u32, dispatcher_queue_sender: &Sender<Dispatch>) {
        let request = Dispatch::Request(self.blockchain.last_notarized().block.length, sender);
        let _ = dispatcher_queue_sender.send(request).await;
    }
}
