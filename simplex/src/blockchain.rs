use bincode::serialize;
use log::info;
use crate::block::{SimplexBlockHeader, NotarizedBlock, SimplexBlock, VoteSignature};
use serde_json::to_string;
use sha2::{Digest, Sha256};
use shared::domain::transaction::Transaction;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const GENESIS_ITERATION: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const INITIAL_FINALIZED_HEIGHT: u32 = 0;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks_";


pub struct Blockchain {
    notarized: Vec<NotarizedBlock>,
    my_node_id: u32,
    to_be_finalized: Vec<u32>, // vec iteration of blocks to be finalized
    to_be_notarized: Vec<NotarizedBlock>,
    finalized_height: u32,
}

impl Blockchain {

    pub fn new(my_node_id: u32) -> Self {
        let mut block_nodes = Vec::new();
        block_nodes.push(NotarizedBlock { block: Self::genesis_block(), signatures: Vec::new(), transactions: Vec::new() });
        Blockchain { notarized: block_nodes, my_node_id, to_be_finalized: Vec::new(), to_be_notarized: Vec::new(), finalized_height: INITIAL_FINALIZED_HEIGHT }
    }

    fn genesis_block() -> SimplexBlockHeader {
        SimplexBlockHeader { hash: None, iteration: GENESIS_ITERATION, length: GENESIS_LENGTH, transactions: Vec::new() }
    }

    pub fn last_notarized(&self) -> &NotarizedBlock {
        match self.notarized.iter().max_by_key(|notarized| notarized.block.iteration) {
            Some(notarized) => notarized,
            None => panic!("Impossible scenario"),
        }
    }

    pub fn hash(block: &NotarizedBlock) -> Vec<u8> {
        let block_data = serialize(&block.block).expect("Failed to serialize Block");
        let mut hasher = Sha256::new();
        hasher.update(&block_data);
        hasher.finalize().to_vec()
    }

    pub fn get_block(&self, iteration: u32) -> Option<&NotarizedBlock> {
        self.notarized.iter().find(|notarized| notarized.block.iteration == iteration)
    }

    pub fn get_next_block(&self, iteration: u32, transactions: Vec<Transaction>) -> SimplexBlock {
        let last = self.last_notarized();
        let hash = Self::hash(&last);
        SimplexBlock::new(Some(hash), iteration, last.block.length + 1, transactions)
    }

    pub fn add_to_be_notarized(&mut self, block: SimplexBlockHeader, transactions: Vec<Transaction>, signatures: Vec<VoteSignature>) {
        self.to_be_notarized.push(NotarizedBlock { block, signatures, transactions })
    }

    pub fn check_for_possible_notarization(&mut self, iteration: u32) -> Option<NotarizedBlock> {
        if let Some(index) = self.to_be_notarized.iter().position(|notarized| notarized.block.iteration == iteration) {
            return Some(self.to_be_notarized.remove(index));
        }
        None
    }

    pub fn is_extendable(&self, block: &SimplexBlock) -> bool {
        let last = self.last_notarized();
        if let Some(_) = self.notarized.iter().find(|notarized|
            Some(Blockchain::hash(notarized)) == block.hash && block.iteration > last.block.iteration && block.length == last.block.length + 1
        ) {
            return true
        }
        false
    }

    pub fn find_parent_block(&self, hash: &Option<Vec<u8>>) -> Option<&NotarizedBlock> {
        self.notarized.iter().find(|notarized| Some(Blockchain::hash(notarized)) == *hash)
    }

    pub async fn notarize(&mut self, block: SimplexBlockHeader, transactions: Vec<Transaction>, signatures: Vec<VoteSignature>) {
        let iteration = block.iteration;
        self.notarized.push(NotarizedBlock { block, signatures, transactions });
        if self.to_be_finalized.contains(&iteration) {
            self.finalize(iteration).await;
            self.to_be_finalized.retain(|iter| *iter != iteration);
        }
    }

    pub fn is_missing(&self, length: u32, iteration: u32) -> bool {
        let last = self.last_notarized();
        last.block.length < length || (last.block.length == length && iteration != last.block.iteration)
    }

    pub async fn get_missing(&self, from_length: u32) -> Vec<NotarizedBlock> {
        let mut missing = Vec::new();
        let last = self.last_notarized();
        for curr_length in from_length ..= last.block.length {
            let mut blocks: Vec<NotarizedBlock> = self.notarized.iter().filter(|notarized| notarized.block.length == curr_length).cloned().collect();
            if blocks.is_empty() && curr_length != GENESIS_LENGTH {
                match self.get_finalized_block(curr_length).await {
                    Some(notarized) => {
                        missing.push(notarized);
                    }
                    None => {
                        break
                    }
                }
            } else {
                missing.append(&mut blocks);
            }
        }
        missing
    }

    pub fn add_to_be_finalized(&mut self, iteration: u32) {
        self.to_be_finalized.push(iteration);
    }

    pub async fn finalize(&mut self, iteration: u32) {
        let blocks: Vec<NotarizedBlock> = self.notarized.iter()
            .filter(|notarized| notarized.block.iteration <= iteration && notarized.block.iteration > self.finalized_height)
            .cloned()
            .collect();

        let mut blocks_to_be_finalized: Vec<NotarizedBlock> = Vec::new();
        let mut last_parent_hash = None;
        for notarized in blocks.iter().rev() {
            if last_parent_hash == Some(Self::hash(notarized)) || last_parent_hash.is_none() {
                last_parent_hash = notarized.block.hash.clone();
                blocks_to_be_finalized.push(notarized.clone());
            }
        }

        let node_id = self.my_node_id;
        tokio::spawn(async move {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(format!("{}{}.ndjson", FINALIZED_BLOCKS_FILENAME, node_id))
                .await
                .expect("Could not open blockchain file");

            for notarized in blocks_to_be_finalized.iter().rev() {
                let block_data = to_string(&notarized).expect("Failed to serialize block") + "\n";
                file.write_all(block_data.as_bytes()).await.expect("Error writing block to file");
            }
        });

        self.finalized_height = iteration;
        self.notarized.retain(|notarized| notarized.block.iteration >= iteration);
    }


    async fn get_finalized_block(&self, length: u32) -> Option<NotarizedBlock> {
        let file = File::open(format!("{}{}.ndjson", FINALIZED_BLOCKS_FILENAME, self.my_node_id)).await.expect("Could not find Blockchain file");
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut current_index = 0;
        while let Some(line) = lines.next_line().await.unwrap_or(None) {
            if current_index == (length - 1) {
                return serde_json::from_str(&line).ok();
            }
            current_index += 1;
        }
        None
    }

    pub fn print(&self) {
        for notarized in self.notarized.iter() {
            info!("[Iteration: {} | Length: {}]", notarized.block.iteration, notarized.block.length);
        }
    }
}
