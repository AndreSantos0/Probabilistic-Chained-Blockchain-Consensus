use crate::blockchain::Blockchain;
use crate::message::SimplexMessage;
use ring::signature::Ed25519KeyPair;
use shared::connection::{accept_connections, connect};
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

const SEED: u64 = 0xFF6DA736EA;
const INITIAL_ITERATION: u32 = 0;
const ITERATION_TIME: u64 = 3;
const MESSAGE_CHANNEL_SIZE: usize = 50;


pub struct SimplexNode {
    environment: Environment,
    iteration: Arc<AtomicU32>,
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
                let (tx, rx) = mpsc::channel::<SimplexMessage>(MESSAGE_CHANNEL_SIZE);
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
                tokio::select! {
                    _ = timer.tick() => {
                        // Timer completed without reset
                        iteration_counter.fetch_add(1, Ordering::SeqCst);
                        match iter_tx.send(IterationAdvance::Timeout).await {
                            Ok(_) => {},
                            Err(_) => return
                        };
                    }
                    _ = reset_rx.recv() => {
                        // Timer was reset
                        iteration_counter.fetch_add(1, Ordering::SeqCst);
                        match iter_tx.send(IterationAdvance::Notarization).await {
                            Ok(_) => {},
                            Err(_) => return
                        };
                    }
                }
            }
        });
    }

    async fn execute_protocol(
        &mut self,
        mut iteration_receiver: Receiver<IterationAdvance>,
        reset_sender: Sender<()>,
        mut message_queue_receiver: Receiver<SimplexMessage>,
        connections: &mut Vec<TcpStream>
    ) {
        let mut rng = StdRng::seed_from_u64(SEED);
        loop {
            tokio::select! {
                Some(val) = iteration_receiver.recv() => {
                    match val {
                        IterationAdvance::Timeout => {},
                        IterationAdvance::Notarization => {}
                    }

                    let epoch = self.iteration.load(Ordering::SeqCst);
                    let leader = self.get_leader(epoch, &mut rng);
                    println!("----------------------");
                    println!("----------------------");
                    println!("Leader is node {} | Epoch: {}", leader, epoch);
                    self.blockchain.print();
                    if leader == self.environment.my_node.id {
                        self.propose(connections).await;
                    }
                }
            }
        }
    }
}
