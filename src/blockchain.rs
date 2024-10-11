use std::fs::{OpenOptions};
use std::io::Write;
use serde_json::to_string;
use sha1::{Digest, Sha1};

use crate::domain::block::Block;
use crate::domain::transaction::Transaction;

const GENESIS_EPOCH: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const N_NODES_FOR_FINALIZATION: usize = 3;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks";


pub struct Blockchain {
    nodes: Vec<Block>,
    delayed: Vec<Block>,
    my_node_id: u32,
}

impl Blockchain {
    pub fn new(my_node_id: u32) -> Self {
        let mut block_nodes = Vec::new();
        block_nodes.push(Self::genesis_block());
        Blockchain { nodes: block_nodes, delayed: Vec::new(), my_node_id }
    }

    fn genesis_block() -> Block {
        Block::new(None, GENESIS_EPOCH, GENESIS_LENGTH, Vec::new())
    }

    fn hash(block: &Block) -> Vec<u8> {
        let block_data = to_string(block).expect("Failed to serialize Block");
        let mut hasher = Sha1::new();
        hasher.update(block_data.as_bytes());
        hasher.finalize().to_vec()
    }

    fn find_previous_block(&self, block: &Block) -> Option<&Block> {
        self.nodes.iter().find(|blockchain_node| Some(Blockchain::hash(blockchain_node)) == block.hash)
    }

    pub fn add_block(&mut self, block: &Block) -> bool {
        match self.find_previous_block(&block) {
            Some(_) => {
                self.nodes.push(block.clone());
                for delayed_block in self.delayed.clone() {
                    let hash = Self::hash(&block);
                    if Some(hash) == delayed_block.hash {
                        self.delayed.retain(|block| block.epoch != delayed_block.epoch || block.epoch < delayed_block.epoch);
                        self.nodes.push(delayed_block.clone());
                        if self.is_finalize(&delayed_block) {
                            self.finalize(&delayed_block);
                            return true
                        }
                    }
                }
                if self.is_finalize(&block) {
                    self.finalize(&block);
                    return true
                }
                false
            },
            None => {
                self.delayed.push(block.clone());
                false
            }
        }
    }

    fn get_longest_chain(&self) -> &Block {
        self.nodes.iter().max_by_key(|blockchain_node| blockchain_node.length).unwrap()
    }

    pub fn get_longest_chain_length(&self) -> u32 {
        self.nodes.iter().max_by_key(|blockchain_node| blockchain_node.length).unwrap().length
    }

    pub fn get_next_block(&self, epoch: u32, transactions: Vec<Transaction>) -> Block {
        let longest_chain_block = self.get_longest_chain();
        let hash = Blockchain::hash(longest_chain_block);
        Block::new(Some(hash), epoch, longest_chain_block.length + 1, transactions)
    }

    fn is_finalize(&self, node: &Block) -> bool {
        let mut count = 0;
        let mut epochs = Vec::new();
        let mut curr_node = node;
        while count < N_NODES_FOR_FINALIZATION {
            match self.find_previous_block(curr_node) {
                Some(previous) => {
                    if curr_node.epoch == GENESIS_EPOCH { return false };
                    epochs.push(curr_node.epoch);
                    count += 1;
                    curr_node = previous;
                } ,
                None => return false
            }
        }
        epochs[0] == epochs[1] + 1 && epochs[1] == epochs[2] + 1
    }

    fn finalize(&mut self, node: &Block) {
        let mut curr_node = node;
        let mut to_finalize = Vec::new();
        while let Some(previous) = self.find_previous_block(curr_node) {
            curr_node = previous;
            to_finalize.push(curr_node)
        }
        to_finalize.reverse();

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(format!("{}{}", FINALIZED_BLOCKS_FILENAME, self.my_node_id)).expect("Could not find Blockchain file");

        for block in to_finalize {
            writeln!(file, "Epoch: {} | Length: {} | Transactions: {:?}", block.epoch, block.length, block.transactions).expect("Error writing block to file");
        }
        self.nodes.retain(|block| block.epoch == node.epoch || block.epoch == node.epoch - 1);
    }

    fn print_blockchain(&self, node: &Block, n_tabs: usize) {
        if n_tabs == 1 {
            println!("[Epoch: {} | Length: {}]", node.epoch, node.length);
        }

        let childs: Vec<Block> = self.nodes.iter().filter(|child| {
            match self.find_previous_block(child) {
                None => false,
                Some(previous) => { previous.epoch == node.epoch }
            }
        }).cloned().collect();

        let tabs = "\t".repeat(n_tabs);
        for child in childs {
            println!("{}[Epoch: {} | Length: {}]", tabs, child.epoch, child.length);
            self.print_blockchain(&child, n_tabs + 1);
        }
    }

    pub fn print(&self) {
        let genesis = self.nodes.iter().min_by_key(|node| node.epoch).unwrap();
        self.print_blockchain(genesis, 1)
    }
}
