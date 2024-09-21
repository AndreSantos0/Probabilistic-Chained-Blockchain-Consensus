use serde_json::to_string;
use sha1::{Digest, Sha1};

use crate::domain::block::Block;
use crate::domain::transaction::Transaction;

const GENESIS_HASH: &str = "0";
const GENESIS_EPOCH: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const N_NODES_FOR_FINALIZATION: usize = 3;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks";


#[derive(Clone)]
struct BlockchainNode {
    block: Block,
    is_finalized: bool,
}

pub struct Blockchain {
    nodes: Vec<BlockchainNode>,
}

impl BlockchainNode {
    fn new(block: Block) -> Self {
        BlockchainNode { block, is_finalized: false }
    }
}

impl Blockchain {
    pub fn new() -> Self {
        let mut block_nodes = Vec::new();
        block_nodes.push(Self::genesis_block());
        Blockchain { nodes: block_nodes }
    }

    fn genesis_block() -> BlockchainNode {
        let genesis_block = Block::new(None, GENESIS_EPOCH, GENESIS_LENGTH, Vec::new());
        BlockchainNode::new(genesis_block)
    }

    fn hash(block: &Block) -> Vec<u8> {
        let block_data = to_string(block).expect("Failed to serialize Block");
        let mut hasher = Sha1::new();
        hasher.update(block_data.as_bytes());
        hasher.finalize().to_vec()
    }

    fn find_previous_block(&self, block: &Block) -> Option<&BlockchainNode> {
        self.nodes.iter().find(|blockchain_node| Some(Blockchain::hash(&blockchain_node.block)) == block.hash)
    }

    pub fn add_block(&mut self, block: Block) {
        match self.find_previous_block(&block) {
            Some(_) => self.nodes.push(BlockchainNode::new(block)),
            None => {}
        }
    }

    fn get_longest_chain(&self) -> &BlockchainNode {
        self.nodes.iter().max_by_key(|blockchain_node| blockchain_node.block.length).unwrap()
    }

    pub fn get_longest_chain_length(&self) -> u32 {
        self.nodes.iter().max_by_key(|blockchain_node| blockchain_node.block.length).unwrap().block.length
    }

    pub fn get_next_block(&self, epoch: u32, transactions: Vec<Transaction>) -> Block {
        let longest_chain = self.get_longest_chain();
        let hash = Blockchain::hash(&longest_chain.block);
        Block::new(Some(hash), epoch, longest_chain.block.length, transactions)
    }

    pub fn print(&self) {
        let curr_length = self.get_longest_chain_length();
        let edge_nodes: Vec<BlockchainNode> = self.nodes.iter().filter(|blockchain_node| blockchain_node.block.length == curr_length).cloned().collect();
        for node in edge_nodes {
            let mut curr_node = &node;
            print!("[Epoch: {} | Finalized: {} | Length: {}]", node.block.epoch, node.is_finalized, node.block.length);
            while let Some(previous) = self.find_previous_block(&curr_node.block) {
                curr_node = previous;
                print!(" <- [Epoch: {} | Finalized: {} | Length: {}]", curr_node.block.epoch, curr_node.is_finalized, curr_node.block.length);
            }
            print!("\n");
        }
    }
}
