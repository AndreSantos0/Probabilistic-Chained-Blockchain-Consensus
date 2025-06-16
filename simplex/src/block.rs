use bincode::serialize;
use sha2::{Digest, Sha256};
use shared::domain::transaction::Transaction;
use std::hash::Hash;


pub type NodeId = u32;
pub type Iteration = u32;

#[derive(Hash, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct SimplexBlock {
    pub hash: Option<Vec<u8>>,
    pub iteration: Iteration,
    pub length: u32,
    pub transactions: Vec<Transaction>,
}

impl SimplexBlock {
    pub fn new(hash: Option<Vec<u8>>, iteration: u32, length: u32, transactions: Vec<Transaction>) -> Self {
        SimplexBlock { hash, iteration, length, transactions }
    }
}

#[derive(Hash, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct VoteSignature {
    pub signature: Vec<u8>,
    pub node: NodeId,
}

#[derive(Hash, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct NotarizedBlock {
    pub header: SimplexBlockHeader,
    pub signatures: Vec<VoteSignature>,
    pub transactions: Vec<Transaction>,
}

#[derive(Hash, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct SimplexBlockHeader {
    pub hash: Option<Vec<u8>>,
    pub iteration: Iteration,
    pub length: u32,
    pub transactions: Vec<u8>,
}

impl From<&SimplexBlock> for SimplexBlockHeader {
    fn from(block: &SimplexBlock) -> Self {
        let transactions_data = serialize(&block.transactions).expect("Failed to serialize Block transactions");
        let mut hasher = Sha256::new();
        hasher.update(&transactions_data);
        let hashed_transactions = hasher.finalize().to_vec();
        SimplexBlockHeader { hash: block.hash.clone(), iteration: block.iteration, length: block.length, transactions: hashed_transactions }
    }
}

pub fn hash(header: &SimplexBlockHeader) -> Vec<u8> {
    let block_data = serialize(header).expect("Failed to serialize block header");
    let mut hasher = Sha256::new();
    hasher.update(&block_data);
    hasher.finalize().to_vec()
}
