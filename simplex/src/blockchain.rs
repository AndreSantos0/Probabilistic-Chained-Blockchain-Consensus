use std::fs::OpenOptions;
use serde_json::to_string;
use sha1::{Digest, Sha1};
use shared::domain::transaction::Transaction;
use crate::block::{NotarizedBlock, SimplexBlock};
use std::io::Write;
use shared::connection::{NodeId, Signature};
use crate::message::SimplexMessage;

const GENESIS_ITERATION: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks";


pub struct Blockchain {
    nodes: Vec<NotarizedBlock>,
    my_node_id: u32,
    to_be_finalized: Vec<u32>, // vec iteration of blocks to be finalized
    delayed_notarized: Vec<NotarizedBlock>,
    finalized_height: u32,
}

impl Blockchain {

    pub fn new(my_node_id: u32) -> Self {
        let mut block_nodes = Vec::new();
        block_nodes.push(NotarizedBlock { block: Self::genesis_block(), signatures: Vec::new() });
        Blockchain { nodes: block_nodes, my_node_id, to_be_finalized: Vec::new(), delayed_notarized: Vec::new(), finalized_height: 0 }
    }

    fn genesis_block() -> SimplexBlock {
        SimplexBlock::new(None, GENESIS_ITERATION, GENESIS_LENGTH, Vec::new())
    }

    pub fn last_notarized(&self) -> SimplexBlock {
        match self.nodes.last() {
            Some(notarized) => notarized.block.clone(),
            None => Self::genesis_block(),
        }
    }

    pub fn hash(block: &SimplexBlock) -> Vec<u8> {
        let block_data = to_string(block).expect("Failed to serialize Block");
        let mut hasher = Sha1::new();
        hasher.update(block_data.as_bytes());
        hasher.finalize().to_vec()
    }

    pub fn get_block(&self, iteration: u32) -> Option<&SimplexBlock> {
        let notarized = self.nodes.iter().find(|notarized| notarized.block.iteration == iteration);
        match notarized {
            None => None,
            Some(notarized) => Some(&notarized.block)
        }
    }

    pub fn get_next_block(&self, iteration: u32, transactions: Vec<Transaction>) -> SimplexBlock {
        let last = self.last_notarized();
        let hash = Self::hash(&last);
        SimplexBlock::new(Some(hash), iteration, last.length + 1, transactions)
    }

    pub fn add_block(&mut self, block: SimplexBlock, signatures: Vec<(SimplexMessage, Signature, NodeId)>) -> bool {
        let last = self.last_notarized();
        match self.get_block(block.iteration) {
            None => {
                if Some(Blockchain::hash(&last)) == block.hash && block.iteration > last.iteration && block.length == last.length + 1 {
                    let iteration = block.iteration;
                    self.nodes.push(NotarizedBlock { block, signatures });
                    if self.to_be_finalized.contains(&iteration) {
                        self.finalize(iteration);
                        self.to_be_finalized.retain(|iter| *iter != iteration);
                    }
                    while !self.delayed_notarized.is_empty() {
                        if let Some(delayed) = self.delayed_notarized.first() {
                            let last = self.last_notarized();
                            if Some(Blockchain::hash(&last)) == delayed.block.hash && delayed.block.iteration > last.iteration && delayed.block.length == last.length + 1 {
                                let delayed_block = self.delayed_notarized.remove(0);
                                let iteration = delayed_block.block.iteration;
                                self.nodes.push(delayed_block);
                                if self.to_be_finalized.contains(&iteration) {
                                    self.finalize(iteration);
                                    self.to_be_finalized.retain(|iter| *iter != iteration);
                                }
                            } else {
                                break
                            }
                        } // else is impossible
                    }
                    true
                } else {
                    self.delayed_notarized.push(NotarizedBlock { block, signatures });
                    false
                }
            }
            Some(_) => false
        }
    }

    pub fn is_extendable(&self, new_block: &SimplexBlock) -> bool {
        let last = self.last_notarized();
        println!("-----");
        println!("Last Block: {:?} | {} | {}", last.hash, last.iteration, last.length);
        println!("Last Hash: {:?}", Self::hash(&last));
        println!("New Block: {:?} | {} | {}", new_block.hash, new_block.iteration, new_block.length);
        println!("-----");
        new_block.hash == Some(Self::hash(&last)) && new_block.length == last.length + 1 && new_block.iteration > last.iteration
    }

    pub fn is_missing(&self, length_observed: u32) -> bool {
        let last = self.last_notarized();
        last.length < length_observed //TODO: Se block tiver apenas length + 1 skippar a fase de pedir os blocos e introduzir logo
    }

    pub fn get_missing(&self, last: SimplexBlock) -> Option<Vec<NotarizedBlock>> {
        let first_missing = self.nodes.iter().find(|notarized| notarized.block.length == last.length + 1);
        match first_missing {
            Some(notarized) => {
                if Some(Self::hash(&last)) == notarized.block.hash {
                    Some(self.nodes.iter().filter(|notarized| notarized.block.length > last.length).cloned().collect())
                } else {
                    None
                }
            },
            None => None
        }
    }

    pub fn add_to_be_finalized(&mut self, iteration: u32) {
        self.to_be_finalized.push(iteration);
    }

    pub fn finalize(&mut self, iteration: u32) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(format!("{}{}", FINALIZED_BLOCKS_FILENAME, self.my_node_id))
            .expect("Could not find Blockchain file");


        let blocks: Vec<&NotarizedBlock> = self.nodes.iter()
            .filter(|notarized| notarized.block.iteration <= iteration && notarized.block.iteration > self.finalized_height)
            .collect();

        for notarized in blocks {
            writeln!(file, "Iteration: {} | Length: {} | Transactions: {:?}", notarized.block.iteration, notarized.block.length, notarized.block.transactions)
                .expect("Error writing block to file");
        }
        self.finalized_height = iteration;
        self.nodes.retain(|notarized| notarized.block.iteration >= iteration);
    }

    pub fn print(&self) {
        for notarized in self.nodes.iter() {
            println!("[Iteration: {} | Length: {}]", notarized.block.iteration, notarized.block.length);
        }
    }
}
