use tokio::fs::{File, OpenOptions};
use serde_json::to_string;
use sha2::{Digest, Sha256};
use shared::domain::transaction::Transaction;
use crate::block::{HashedSimplexBlock, NotarizedBlock, SimplexBlock, VoteSignature};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const GENESIS_ITERATION: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const INITIAL_FINALIZED_HEIGHT: u32 = 0;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks_";


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
        block_nodes.push(NotarizedBlock { block: Self::genesis_block(), signatures: Vec::new(), transactions: Vec::new() });
        Blockchain { nodes: block_nodes, my_node_id, to_be_finalized: Vec::new(), delayed_notarized: Vec::new(), finalized_height: INITIAL_FINALIZED_HEIGHT }
    }

    fn genesis_block() -> HashedSimplexBlock {
        HashedSimplexBlock { hash: None, iteration: GENESIS_ITERATION, length: GENESIS_LENGTH, transactions: Vec::new() }
    }

    pub fn last_notarized_length(&self) -> u32 {
        match self.nodes.last() {
            Some(notarized) => notarized.block.length,
            None => GENESIS_LENGTH,
        }
    }

    pub fn last_notarized(&self) -> NotarizedBlock {
        match self.nodes.last() {
            Some(notarized) => notarized.clone(),
            None => NotarizedBlock { block: Self::genesis_block(), signatures: Vec::new(), transactions: Vec::new() },
        }
    }

    pub fn hash(block: &NotarizedBlock) -> Vec<u8> {
        let block_data = to_string(&block.block).expect("Failed to serialize Block");
        let mut hasher = Sha256::new();
        hasher.update(block_data.as_bytes());
        hasher.finalize().to_vec()
    }

    pub fn has_block(&self, iteration: u32) -> bool {
        let notarized = self.nodes.iter().find(|notarized| notarized.block.iteration == iteration);
        match notarized {
            None => false,
            Some(_) => true,
        }
    }

    pub fn get_next_block(&self, iteration: u32, transactions: Vec<Transaction>) -> SimplexBlock {
        let last = self.last_notarized();
        let hash = Self::hash(&last);
        SimplexBlock::new(Some(hash), iteration, last.block.length + 1, transactions)
    }

    fn is_delayed(&self, length: u32) -> bool {
        let last = self.last_notarized();
        length > last.block.length + 1
    }

    pub async fn add_block(&mut self, block: HashedSimplexBlock, transactions: Vec<Transaction>, signatures: Vec<VoteSignature>) -> (u32, bool) {
        let last = self.last_notarized();
        let mut n_added = 0;
        if Some(Blockchain::hash(&last)) == block.hash && block.iteration > last.block.iteration && block.length == last.block.length + 1 {
            let iteration = block.iteration;
            self.nodes.push(NotarizedBlock { block, signatures, transactions });
            n_added += 1;
            if self.to_be_finalized.contains(&iteration) {
                self.finalize(iteration).await;
                self.to_be_finalized.retain(|iter| *iter != iteration);
            }
            while !self.delayed_notarized.is_empty() {
                if let Some(delayed) = self.delayed_notarized.first() {
                    let last = self.last_notarized();
                    if Some(Blockchain::hash(&last)) == delayed.block.hash && delayed.block.iteration > last.block.iteration && delayed.block.length == last.block.length + 1 {
                        let delayed_block = self.delayed_notarized.remove(0);
                        let iteration = delayed_block.block.iteration;
                        self.nodes.push(delayed_block);
                        n_added += 1;
                        if self.to_be_finalized.contains(&iteration) {
                            self.finalize(iteration).await;
                            self.to_be_finalized.retain(|iter| *iter != iteration);
                        }
                    } else {
                        break
                    }
                } // else is impossible
            }
            (n_added, false)
        } else {
            if self.is_delayed(block.length) {
                self.delayed_notarized.push(NotarizedBlock { block, signatures, transactions });
                return (n_added, true)
            }
            (n_added, false)
        }
    }

    pub fn is_extendable(&self, new_block: &SimplexBlock) -> bool {
        let last = self.last_notarized();
        new_block.hash == Some(Self::hash(&last)) && new_block.length == last.block.length + 1 && new_block.iteration > last.block.iteration
    }

    pub fn is_missing(&self, length_observed: u32) -> bool {
        let last = self.last_notarized();
        last.block.length < length_observed
    }

    pub async fn get_missing(&self, from: u32) -> Vec<NotarizedBlock> {
        let mut missing = Vec::new();
        let last = self.last_notarized();
        for length in from + 1 ..= last.block.length {
            match self.nodes.iter().find(|notarized| notarized.block.length == length) {
                Some(notarized) => {
                    missing.push(notarized.clone());
                }
                None => {
                    match self.get_finalized_block(length).await {
                        Some(notarized) => {
                            missing.push(notarized);
                        }
                        None => {
                            break
                        }
                    }
                }
            }
        }
        missing
    }

    pub fn add_to_be_finalized(&mut self, iteration: u32) {
        self.to_be_finalized.push(iteration);
    }

    pub async fn finalize(&mut self, iteration: u32) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .append(true)
            .open(format!("{}{}.ndjson", FINALIZED_BLOCKS_FILENAME, self.my_node_id))
            .await.expect("Could not find Blockchain file");

        let blocks: Vec<&NotarizedBlock> = self.nodes.iter()
            .filter(|notarized| notarized.block.iteration <= iteration && notarized.block.iteration > self.finalized_height)
            .collect();

        for notarized in blocks {
            let block_data = to_string(&notarized).expect("Failed to serialize Block") + "\n";
            file.write_all(block_data.as_bytes()).await.expect("Error writing block to file");
        }

        self.finalized_height = iteration;
        self.nodes.retain(|notarized| notarized.block.iteration >= iteration);
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
        for notarized in self.nodes.iter() {
            println!("[Iteration: {} | Length: {}]", notarized.block.iteration, notarized.block.length);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_finalize_file() {
        let length = 1;
        let file = File::open(format!("{}.ndjson", FINALIZED_BLOCKS_FILENAME)).await.expect("Could not find Blockchain file");
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut current_index = 0;
        while let Some(line) = lines.next_line().await.unwrap_or(None) {
            if current_index == (length - 1) {
                println!("{:?}", serde_json::from_str::<NotarizedBlock>(&line).ok().unwrap());
                return;
            }
            current_index += 1;
        }
    }
}