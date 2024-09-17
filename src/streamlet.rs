use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time;
use crate::my_node::{NodeSocket, ServerSocket};

const SEED: i32 = 0xFF6DA736EA;
const INITIAL_EPOCH: u32 = 0;
const EPOCH_TIME: u64 = 3;
const CONFUSION_START: u32 = 1;
const CONFUSION_DURATION: u32 = 2;

pub struct Streamlet<'a> {
    my_node_id: u32,
    n_nodes: u32,
    node_sockets: &'a Vec<NodeSocket>,
    server_sockets: &'a Vec<ServerSocket>,
    epoch: u32,
    is_new_epoch: Arc<AtomicBool>,
}

impl<'a> Streamlet<'a> {

    pub fn new(my_node_id: u32, n_nodes: u32, node_sockets: &'a Vec<NodeSocket>, server_sockets: &'a Vec<ServerSocket>) -> Streamlet<'a> {
        let mut epoch = INITIAL_EPOCH;
        let is_new_epoch = Arc::new(AtomicBool::new(false));

        Streamlet {
            my_node_id,
            n_nodes,
            node_sockets,
            server_sockets,
            epoch,
            is_new_epoch,
        }
    }

    pub fn start_protocol(&mut self) {
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(EPOCH_TIME));
            loop {
                interval.tick().await;
                self.epoch += 1;
                loop {
                    match self.is_new_epoch.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
                        Ok(_) => break,
                        Err(_) => { }
                    }
                }
            }
        });
        self.listen();
        self.execute_protocol();
    }

    fn execute_protocol() {

    }

    fn get_leader() {

    }

    fn listen() {

    }
}
