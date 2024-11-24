use std::fs::OpenOptions;
use serde_json::to_string;
use sha1::{Digest, Sha1};
use shared::domain::transaction::Transaction;
use crate::block::SimplexBlock;
use std::io::Write;

const GENESIS_EPOCH: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks";


pub struct Blockchain {
    nodes: Vec<SimplexBlock>,
    my_node_id: u32,
}

impl Blockchain {

    pub fn new(my_node_id: u32) -> Self {
        let mut block_nodes = Vec::new();
        block_nodes.push(Self::genesis_block());
        Blockchain { nodes: block_nodes, my_node_id }
    }

    fn genesis_block() -> SimplexBlock {
        SimplexBlock::new(None, GENESIS_EPOCH, GENESIS_LENGTH, Vec::new())
    }

    pub fn last_notarized(&self) -> SimplexBlock {
        match self.nodes.last() {
            Some(block) => block.clone(),
            None => Self::genesis_block(),
        }
    }

    pub fn hash(block: &SimplexBlock) -> Vec<u8> {
        let block_data = to_string(block).expect("Failed to serialize Block");
        let mut hasher = Sha1::new();
        hasher.update(block_data.as_bytes());
        hasher.finalize().to_vec()
    }

    pub fn get_next_block(&self, iteration: u32, transactions: Vec<Transaction>) -> SimplexBlock {
        let last = self.nodes.last().unwrap();
        let hash = Self::hash(last);
        SimplexBlock::new(Some(hash), iteration, last.length + 1, transactions)
    }

    pub fn add_block(&mut self, block: SimplexBlock) {
        let last = self.last_notarized();
        if Some(Blockchain::hash(&last)) == block.hash {
            self.nodes.push(block);
        }
    }

    pub fn is_extendable(&self, new_block: &SimplexBlock) -> bool {
        let last = self.nodes.last().unwrap();
        new_block.hash == Some(Self::hash(last)) && new_block.length == last.length + 1
    }

    pub fn is_missing(&self, block: SimplexBlock) -> bool {
        let last = self.last_notarized();
        last.length < block.length //TODO: Se block tiver apenas length + 1 skippar a fase de pedir os blocos e introduzir logo
    }

    pub fn get_missing(&self, last: SimplexBlock) -> Option<Vec<SimplexBlock>> {
        let from = last.iteration;
        let first_missing = self.nodes.iter().find(|block| block.length == last.iteration + 1);
        match first_missing {
            Some(block) => {
                if Some(Self::hash(&last)) == block.hash {
                    Some(self.nodes.iter().filter(|block| block.iteration > from).cloned().collect())
                } else {
                    None
                }
            },
            None => None
        }
    }

    pub fn finalize(&mut self, iteration: u32) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(format!("{}{}", FINALIZED_BLOCKS_FILENAME, self.my_node_id))
            .expect("Could not find Blockchain file");

        let block = self.nodes.iter().find(|block| block.iteration == iteration);
        match block {
            Some(block) => writeln!(file, "Iteration: {} | Length: {} | Transactions: {:?}", block.iteration, block.length, block.transactions).expect("Error writing block to file"),
            None => return
        }

        self.nodes.retain(|block| block.iteration > iteration);
    }

    pub fn print(&self) {
        for block in self.nodes.iter() {
            println!("[Iteration: {} | Length: {}]", block.iteration, block.length);
        }
    }
}
