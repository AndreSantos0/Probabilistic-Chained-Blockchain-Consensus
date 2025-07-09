use std::collections::HashMap;
use crate::block::{Epoch, StreamletBlock};
use serde_json::to_string;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::Sender;
use shared::domain::transaction::Transaction;

const GENESIS_EPOCH: u32 = 0;
const GENESIS_LENGTH: u32 = 0;

pub struct Blockchain {
    nodes: HashMap<Vec<u8>, StreamletBlock>,
    rushed: Vec<StreamletBlock>,
    finalized_height: u32,
}

impl Blockchain {

    pub fn new() -> Self {
        let mut block_nodes = HashMap::new();
        let genesis = Self::genesis_block();
        block_nodes.insert(Self::hash(&genesis), genesis);
        Blockchain { nodes: block_nodes, rushed: Vec::new(), finalized_height: 0 }
    }

    fn genesis_block() -> StreamletBlock {
        StreamletBlock::new(Vec::new(), GENESIS_EPOCH, GENESIS_LENGTH, Vec::new())
    }

    fn hash(block: &StreamletBlock) -> Vec<u8> {
        let block_data = to_string(block).expect("Failed to serialize Block");
        let mut hasher = Sha256::new();
        hasher.update(block_data.as_bytes());
        hasher.finalize().to_vec()
    }

    fn find_parent_block(&self, block: &StreamletBlock) -> Option<&StreamletBlock> {
        let p = self.nodes.get(&block.hash);
        if let Some(b) = p {
            assert_eq!(b.length + 1, block.length);
        }
        p
    }

    pub async fn handle_notarization(&mut self, block: &StreamletBlock, finalize_sender: &Sender<Vec<StreamletBlock>>) -> Vec<Epoch> {
        match self.find_parent_block(block) {
            Some(_) => {
                self.nodes.insert(Self::hash(block), block.clone());

                while let Some(index) = self.rushed.iter().position(|rushed| self.find_parent_block(rushed).is_some()) {
                    println!("{}", index);
                    let rushed = self.rushed.remove(index);
                    self.nodes.insert(Self::hash(&rushed), rushed);
                }

                self.try_finalize(finalize_sender).await
            },
            None => {
                self.rushed.push(block.clone());
                vec![]
            }
        }
    }

    pub fn get_longest_chain(&self) -> &StreamletBlock {
        self.nodes.iter().max_by_key(|(_, block)| block.length).unwrap().1
    }

    pub fn get_next_block(&self, epoch: u32, transactions: Vec<Transaction>) -> StreamletBlock {
        let longest_chain_block = self.get_longest_chain();
        let hash = Blockchain::hash(longest_chain_block);
        StreamletBlock::new(hash, epoch, longest_chain_block.length + 1, transactions)
    }

    async fn try_finalize(&mut self, finalize_sender: &Sender<Vec<StreamletBlock>>) -> Vec<Epoch> {
        let mut to_finalize = None;

        for block in self.nodes.values() {
            let parent = self.find_parent_block(block);
            let grandparent = parent.and_then(|p| self.find_parent_block(p));

            if let (Some(p), Some(gp)) = (parent, grandparent) {
                if block.epoch == p.epoch + 1 && p.epoch == gp.epoch + 1 {
                    let parent_hash = Blockchain::hash(p);
                    let block_epoch = block.epoch;
                    let parent_epoch = p.epoch;
                    let parent_length = p.length;
                    let chain = self.get_chain(parent_hash);
                    to_finalize = Some((chain, block_epoch, parent_epoch, parent_length));
                    break;
                }
            }
        }

        if let Some((chain, block_epoch, parent_epoch, parent_length)) = to_finalize {
            let finalized_epochs = chain.iter().map(|block| block.epoch).collect();
            let _ = finalize_sender.send(chain).await;
            self.finalized_height = parent_length;
            self.nodes.retain(|_, b| b.epoch == block_epoch || b.epoch == parent_epoch);
            finalized_epochs
        } else {
            vec![]
        }
    }

    fn get_chain(&mut self, mut current_hash: Vec<u8>) -> Vec<StreamletBlock> {
        let mut chain = Vec::new();
        if let Some(block) = self.nodes.get(&current_hash) {
            current_hash = block.hash.clone();
            chain.push(block.clone());
        }

        while let Some(block) = self.nodes.remove(&current_hash) {
            current_hash = block.hash.clone();
            if block.length > self.finalized_height {
                chain.push(block);
            }
        }

        chain.reverse();
        chain
    }

    fn print_blockchain(&self, node: &StreamletBlock, n_tabs: usize) {
        if n_tabs == 1 {
            println!("[Epoch: {} | Length: {}]", node.epoch, node.length);
        }

        let childs: Vec<&StreamletBlock> = self.nodes.iter().filter(|(_, child)| {
            match self.find_parent_block(child) {
                None => false,
                Some(previous) => { previous.epoch == node.epoch }
            }
        }).map(|(_, block)| block).collect();

        let tabs = "\t".repeat(n_tabs);
        for child in childs {
            println!("{}[Epoch: {} | Length: {}]", tabs, child.epoch, child.length);
            self.print_blockchain(&child, n_tabs + 1);
        }
    }

    pub fn print(&self) {
        let genesis = self.nodes.iter().min_by_key(|(_, block)| block.length).unwrap().1;
        self.print_blockchain(genesis, 1)
    }
}
