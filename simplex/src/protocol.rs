use crate::block::{SimplexBlockHeader, NodeId, SimplexBlock, NotarizedBlock, Iteration};
use crate::message::{Dispatch, SimplexMessage};
use shared::domain::environment::Environment;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use ed25519_dalek::{Keypair, PublicKey};
use log::{error};
use serde_json::to_string;
use sha2::{Digest, Sha256};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{interval, sleep, Duration};

pub enum ProtocolMode {
    Practical,
    Probabilistic
}

pub struct Latency {
    pub start: SystemTime,
    pub finalization: Option<SystemTime>,
}

pub const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks_";

pub trait Protocol {

    const EXECUTION_TIME_SECS: u64 = 60;
    const MESSAGE_CHANNEL_SIZE: usize = 1000;
    const RESET_TIMER_CHANNEL_SIZE: usize = 100;
    const SOCKET_BINDING_DELAY: u64 = 5;
    const GENESIS_ITERATION: u32 = 0;
    const GENESIS_LENGTH: u32 = 0;
    const INITIAL_FINALIZED_HEIGHT: u32 = 0;
    const INITIAL_ITERATION: u32 = 1;
    const ITERATION_TIME: u64 = 5;
    const PROPOSALS_BEFORE_COUNTDOWN: usize = 50;

    type Message: SimplexMessage;


    fn new(environment: Environment, public_keys: HashMap<u32, PublicKey>, private_key: Keypair) -> Self;
    fn get_environment(&self) -> &Environment;
    fn get_iteration(&self) -> &Arc<AtomicU32>;
    fn get_is_timeout(&self) -> &Arc<AtomicBool>;
    fn get_finalized_blocks(&self) -> usize;
    fn get_finalization_timestamps(&self) -> &HashMap<Iteration, Latency>;

    fn get_finalization_time(&self) -> f64 {
        let mut total = Duration::from_millis(0);
        let mut count = 0.0;

        for latency in self.get_finalization_timestamps().values() {
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

    async fn start(&mut self) {
        let address = format!("{}:{}", self.get_environment().my_node.host, self.get_environment().my_node.port);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let (consumer_queue_sender, consumer_queue_receiver) = mpsc::channel::<(NodeId, Self::Message)>(Self::MESSAGE_CHANNEL_SIZE);
                let (dispatcher_queue_sender, dispatcher_queue_receiver) = mpsc::channel::<Dispatch>(Self::MESSAGE_CHANNEL_SIZE);
                let (reset_timer_sender, reset_timer_receiver) = mpsc::channel::<()>(Self::RESET_TIMER_CHANNEL_SIZE);
                let (finalize_sender, finalize_receiver) = mpsc::channel::<Vec<NotarizedBlock>>(Self::MESSAGE_CHANNEL_SIZE);
                sleep(Duration::from_secs(Self::SOCKET_BINDING_DELAY)).await;
                let connections = self.connect(consumer_queue_sender.clone(), listener).await;
                self.start_message_dispatcher(&consumer_queue_sender, dispatcher_queue_receiver, connections).await;
                self.start_iteration_timer(dispatcher_queue_sender.clone(), reset_timer_receiver).await;
                self.start_finalize_task(finalize_receiver).await;
                self.execute_protocol(consumer_queue_receiver, dispatcher_queue_sender, reset_timer_sender, finalize_sender).await;
            }
            Err(_) => { error!("[Node {}] Failed to bind local port", self.get_environment().my_node.id) }
        };
    }

    async fn connect(&self, message_queue_sender: Sender<(NodeId, Self::Message)>, listener: TcpListener) -> Vec<Option<TcpStream>>;

    async fn start_message_dispatcher(&self, message_queue_sender: &Sender<(NodeId, Self::Message)>, dispatcher_queue_receiver: Receiver<Dispatch>, connections: Vec<Option<TcpStream>>);

    async fn start_iteration_timer(&self, dispatcher_queue_sender: Sender<Dispatch>, mut reset_timer_receiver: Receiver<()>) {
        let iteration_counter = self.get_iteration().clone();
        let is_timeout = self.get_is_timeout().clone();
        let mut timer = interval(Duration::from_secs(Self::ITERATION_TIME));
        timer.tick().await;
        tokio::spawn(async move {
            loop {
                let iteration = iteration_counter.load(Ordering::Acquire);
                tokio::select! {
                    _ = timer.tick() => {
                        if iteration == iteration_counter.load(Ordering::Acquire) {
                            is_timeout.store(true, Ordering::Release);
                            let timeout = Self::create_timeout(iteration + 1);
                            let _ = dispatcher_queue_sender.send(timeout).await;
                            reset_timer_receiver.recv().await;
                            timer = interval(Duration::from_secs(Self::ITERATION_TIME));
                            timer.tick().await;
                        }
                    }
                    _ = reset_timer_receiver.recv() => {
                        timer = interval(Duration::from_secs(Self::ITERATION_TIME));
                        timer.tick().await;
                    }
                }
            }
        });
    }

    async fn start_finalize_task(&self, mut finalize_receiver: Receiver<Vec<NotarizedBlock>>) {
        let my_node_id = self.get_environment().my_node.id;
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
        mut consumer_queue_receiver: Receiver<(u32, Self::Message)>,
        dispatcher_queue_sender: Sender<Dispatch>,
        reset_timer_sender: Sender<()>,
        finalize_sender: Sender<Vec<NotarizedBlock>>,
    ) {
        let mut n_proposals = 0;
        self.handle_iteration_advance(&dispatcher_queue_sender, &reset_timer_sender, &finalize_sender).await;

        while n_proposals < Self::PROPOSALS_BEFORE_COUNTDOWN {
            if let Some((sender, message)) = consumer_queue_receiver.recv().await {
                if message.is_propose() {
                    n_proposals += 1;
                }
                self.handle_message(sender, message, &dispatcher_queue_sender, &reset_timer_sender, &finalize_sender).await;
            }
        }

        let start = tokio::time::Instant::now();
        while start.elapsed() < Duration::from_secs(Self::EXECUTION_TIME_SECS) {
            if let Some((sender, message)) = consumer_queue_receiver.recv().await {
                self.handle_message(sender, message, &dispatcher_queue_sender, &reset_timer_sender, &finalize_sender).await;
            }
        }

        let finalized_blocks = if self.get_finalized_blocks() > Self::PROPOSALS_BEFORE_COUNTDOWN {
            self.get_finalized_blocks() - Self::PROPOSALS_BEFORE_COUNTDOWN
        } else {
            self.get_finalized_blocks()
        };

        println!("Blocks finalized: {} ", finalized_blocks);
        println!("Average finalization time (ms): {}", self.get_finalization_time());
        std::process::exit(0);
    }

    async fn handle_iteration_advance(&mut self, dispatcher_queue_sender: &Sender<Dispatch>, reset_timer_sender: &Sender<()>, finalize_sender: &Sender<Vec<NotarizedBlock>>);

    fn get_leader(n_nodes: usize, iteration: u32) -> u32 {
        let mut hasher = Sha256::new();
        hasher.update(&iteration.to_le_bytes());
        let hash = hasher.finalize();
        let hash_u64 = u64::from_le_bytes(hash[0..8].try_into().expect("Invalid slice length"));
        (hash_u64 % n_nodes as u64) as u32
    }

    async fn handle_message(&mut self, sender: u32, message: Self::Message, dispatcher_queue_sender: &Sender<Dispatch>, reset_timer_sender: &Sender<()>, finalize_sender: &Sender<Vec<NotarizedBlock>>);

    fn create_proposal(&self, block: SimplexBlock) -> Dispatch;
    fn create_timeout(next_iteration: u32) -> Dispatch;
    fn create_vote(iteration: u32, block: SimplexBlockHeader) -> Dispatch;
    fn create_finalize(iteration: u32) -> Dispatch;
    fn get_proposal(&self, iteration: u32) -> Option<&SimplexBlockHeader>;
    fn get_timeouts(&self, iteration: u32) -> usize;
    fn clear_timeouts(&mut self, iteration: u32);
}
