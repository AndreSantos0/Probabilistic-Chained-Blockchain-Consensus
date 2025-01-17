use std::hash::Hash;
use serde_json::to_string;
use sha2::{Digest, Sha256};
use shared::domain::transaction::Transaction;


pub type NodeId = u32;

#[derive(Hash, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct SimplexBlock {
    pub hash: Option<Vec<u8>>,
    pub iteration: u32,
    pub length: u32,
    pub transactions: Vec<Transaction>,
}

impl SimplexBlock {
    pub fn new(hash: Option<Vec<u8>>, iteration: u32, length: u32, transactions: Vec<Transaction>) -> Self {
        SimplexBlock { hash, iteration, length, transactions }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct VoteSignature {
    pub signature: Vec<u8>,
    pub node: NodeId,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct NotarizedBlock {
    pub block: HashedSimplexBlock,
    pub signatures: Vec<VoteSignature>,
    pub transactions: Vec<Transaction>,
}

#[derive(Hash, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct HashedSimplexBlock {
    pub hash: Option<Vec<u8>>,
    pub iteration: u32,
    pub length: u32,
    pub transactions: Vec<u8>,
}

impl From<&SimplexBlock> for HashedSimplexBlock {
    fn from(block: &SimplexBlock) -> Self {
        let transactions_data = to_string(&block.transactions).expect("Failed to serialize Block transactions");
        let mut hasher = Sha256::new();
        hasher.update(transactions_data.as_bytes());
        let hashed_transactions = hasher.finalize().to_vec();
        HashedSimplexBlock { hash: block.hash.clone(), iteration: block.iteration, length: block.length, transactions: hashed_transactions }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct HashedNotarizedBlock {
    pub block: HashedSimplexBlock,
    pub signatures: Vec<VoteSignature>,
}
