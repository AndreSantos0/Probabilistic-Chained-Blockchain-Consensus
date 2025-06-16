use bincode::serialize;
use log::info;
use crate::block::{SimplexBlockHeader, NotarizedBlock, SimplexBlock, VoteSignature, BlockchainBlock};
use sha2::{Digest, Sha256};
use shared::domain::transaction::Transaction;
use tokio::fs::{File};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::Sender;
use crate::message::Dispatch;
use crate::protocol::FINALIZED_BLOCKS_FILENAME;

const GENESIS_ITERATION: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const INITIAL_FINALIZED_HEIGHT: u32 = 0;

/*
pub struct Blockchain {
    chain: Vec<BlockchainBlock>,
    my_node_id: u32,
    finalized_height: u32,
}

impl Blockchain {

    pub fn new(my_node_id: u32) -> Self {
        let mut block_nodes = Vec::new();
        block_nodes.push(BlockchainBlock { header: Self::genesis_block(), signatures: Vec::new() });
        Blockchain { chain: block_nodes, my_node_id, finalized_height: INITIAL_FINALIZED_HEIGHT }
    }

    fn genesis_block() -> SimplexBlockHeader {
        SimplexBlockHeader { hash: None, iteration: GENESIS_ITERATION, length: GENESIS_LENGTH, transactions: Vec::new() }
    }

    pub fn last_notarized(&self) -> &BlockchainBlock {
        match self.chain.iter().max_by_key(|notarized| notarized.header.iteration) {
            Some(notarized) => notarized,
            None => panic!("Impossible scenario"),
        }
    }

    pub fn hash(block: &BlockchainBlock) -> Vec<u8> {
        let block_data = serialize(&block.header).expect("Failed to serialize Block");
        let mut hasher = Sha256::new();
        hasher.update(&block_data);
        hasher.finalize().to_vec()
    }

    pub fn get_block(&self, iteration: u32) -> Option<&BlockchainBlock> {
        self.chain.iter().find(|notarized| notarized.header.iteration == iteration)
    }

    pub fn get_next_block(&self, iteration: u32, transactions: Vec<Transaction>) -> SimplexBlock {
        let last = self.last_notarized();
        let hash = Self::hash(&last);
        SimplexBlock::new(Some(hash), iteration, last.header.length + 1, transactions)
    }

    pub fn is_extendable(&self, block: &SimplexBlockHeader) -> bool {
        let last = self.last_notarized();
        if let Some(_) = self.chain.iter().find(|notarized|
            Some(Blockchain::hash(notarized)) == block.hash && block.iteration > last.header.iteration && block.length == last.header.length + 1
        ) {
            return true
        }
        false
    }

    pub fn find_parent_block(&self, hash: &Option<Vec<u8>>) -> Option<&BlockchainBlock> {
        self.chain.iter().rev().find(|notarized| Some(Blockchain::hash(notarized)) == *hash)
    }

    pub fn notarize(&mut self, block: SimplexBlockHeader, signatures: Vec<VoteSignature>) {
        self.chain.push(BlockchainBlock { header: block, signatures });
    }

    pub fn is_missing(&self, length: u32, iteration: u32) -> bool {
        let last = self.last_notarized();
        last.header.length < length || (last.header.length == length && iteration != last.header.iteration)
    }


    pub async fn get_missing(&self, from_length: u32, sender: u32, dispatcher_queue_sender: &Sender<Dispatch>) {
        let dispatcher_queue_sender = dispatcher_queue_sender.clone();
        let last_length = self.last_notarized().header.length;
        let notarized = self.chain.clone();
        let node_id = self.my_node_id;
        tokio::spawn(async move {
            let mut missing = Vec::new();
            for curr_length in from_length ..= last_length {
                let mut blocks: Vec<NotarizedBlock> = notarized.iter().filter(|notarized| notarized.header.length == curr_length).cloned().collect();
                if blocks.is_empty() && curr_length != GENESIS_LENGTH {
                    match get_finalized_block(node_id, curr_length).await {
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

            if missing.is_empty() { return }
            let reply = Dispatch::Reply(missing, sender);
            let _ = dispatcher_queue_sender.send(reply).await;
        });
    }


    pub async fn finalize(&mut self, iteration: u32) -> Vec<BlockchainBlock> {
        let mut blocks_to_be_finalized: Vec<BlockchainBlock> = Vec::new();
        for notarized in self.chain.iter().rev().filter(|block| block.header.iteration <= iteration && block.header.iteration > self.finalized_height) {
            let last = blocks_to_be_finalized.last();
            match last {
                None => blocks_to_be_finalized.push(notarized.clone()),
                Some(block) => {
                    let hash = Self::hash(notarized);
                    if block.header.hash == Some(hash) {
                        blocks_to_be_finalized.push(notarized.clone());
                    }
                }
            }
        }

        self.finalized_height = iteration;
        self.chain.retain(|notarized| notarized.header.iteration >= iteration);
        blocks_to_be_finalized.into_iter().rev().collect()
    }

}

async fn get_finalized_block(node_id: u32, length: u32) -> Option<NotarizedBlock> {
    let file = File::open(format!("{}{}.ndjson", FINALIZED_BLOCKS_FILENAME, node_id)).await.expect("Could not find Blockchain file");
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
 */