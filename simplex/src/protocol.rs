use crate::block::{SimplexBlockHeader, NodeId, SimplexBlock};
use crate::message::{Dispatch, SimplexMessage};
use ring::signature::{Ed25519KeyPair, UnparsedPublicKey};
use shared::domain::environment::Environment;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use log::{error};
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{interval, sleep, Duration};

pub enum ProtocolMode {
    Practical,
    Probabilistic
}

pub trait Protocol {

    const INITIAL_ITERATION: u32 = 1;
    const ITERATION_TIME: u64 = 5;
    const MESSAGE_CHANNEL_SIZE: usize = 1000;
    const RESET_TIMER_CHANNEL_SIZE: usize = 100;
    const SOCKET_BINDING_DELAY: u64 = 5;

    type Message: SimplexMessage;

    fn new(environment: Environment, public_keys: HashMap<u32, UnparsedPublicKey<Vec<u8>>>, private_key: Ed25519KeyPair) -> Self;
    fn get_environment(&self) -> &Environment;
    fn get_iteration(&self) -> &Arc<AtomicU32>;
    fn get_is_timeout(&self) -> &Arc<AtomicBool>;
    fn get_finalized_blocks(&self) -> u32;

    async fn start(&mut self)
    where
        Self: Sized, <Self as Protocol>::Message: 'static
    {
        let address = format!("{}:{}", self.get_environment().my_node.host, self.get_environment().my_node.port);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let (consumer_queue_sender, consumer_queue_receiver) = mpsc::channel::<(NodeId, Self::Message)>(Self::MESSAGE_CHANNEL_SIZE);
                let (dispatcher_queue_sender, dispatcher_queue_receiver) = mpsc::channel::<Dispatch>(Self::MESSAGE_CHANNEL_SIZE);
                let (reset_timer_sender, reset_timer_receiver) = mpsc::channel::<()>(Self::RESET_TIMER_CHANNEL_SIZE);
                sleep(Duration::from_secs(Self::SOCKET_BINDING_DELAY)).await;
                let connections = self.connect(consumer_queue_sender.clone(), listener).await;
                self.start_message_dispatcher(&consumer_queue_sender, dispatcher_queue_receiver, connections).await;
                self.start_iteration_timer(dispatcher_queue_sender.clone(), reset_timer_receiver).await;
                self.execute_protocol(consumer_queue_receiver, dispatcher_queue_sender, reset_timer_sender).await;
            }
            Err(_) => { error!("[Node {}] Failed to bind local port", self.get_environment().my_node.id) }
        };
    }

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

    fn get_leader(n_nodes: usize, iteration: u32) -> u32 {
        let mut hasher = Sha256::new();
        hasher.update(&iteration.to_le_bytes());
        let hash = hasher.finalize();
        let hash_u64 = u64::from_le_bytes(hash[0..8].try_into().expect("Invalid slice length"));
        (hash_u64 % n_nodes as u64) as u32
    }

    async fn handle_iteration_advance(&mut self, dispatcher_queue_sender: &Sender<Dispatch>);

    async fn execute_protocol(
        &mut self,
        mut consumer_queue_receiver: Receiver<(u32, Self::Message)>,
        dispatcher_queue_sender: Sender<Dispatch>,
        reset_timer_sender: Sender<()>,
    )
    where
        Self: Sized,
    {
        let start = tokio::time::Instant::now();
        self.handle_iteration_advance(&dispatcher_queue_sender).await;
        while start.elapsed() < Duration::from_secs(60) {
            if let Some((sender, message)) = consumer_queue_receiver.recv().await {
                self.handle_message(sender, message, &dispatcher_queue_sender, &reset_timer_sender).await;
            }
        }
        println!("{} blocks finalized", self.get_finalized_blocks());
        std::process::exit(0);
    }

    async fn connect(&self, message_queue_sender: Sender<(NodeId, Self::Message)>, listener: TcpListener) -> Vec<Option<TcpStream>>;
    fn create_proposal(&self, block: SimplexBlock) -> Dispatch;
    fn create_timeout(next_iteration: u32) -> Dispatch;
    fn create_vote(iteration: u32, block: SimplexBlockHeader) -> Dispatch;
    fn create_finalize(iteration: u32) -> Dispatch;
    fn get_proposal_block(&self, iteration: u32) -> Option<&SimplexBlock>;
    fn get_timeouts(&self, iteration: u32) -> usize;
    fn clear_timeouts(&mut self, iteration: u32);
    async fn handle_message(&mut self, sender: u32, message: Self::Message, dispatcher_queue_sender: &Sender<Dispatch>, reset_timer_sender: &Sender<()>);
    async fn start_message_dispatcher(&self, message_queue_sender: &Sender<(NodeId, Self::Message)>, dispatcher_queue_receiver: Receiver<Dispatch>, connections: Vec<Option<TcpStream>>);
}
