use shared::connection::{NodeId, Signature};
use shared::domain::transaction::Transaction;
use crate::message::SimplexMessage;

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
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
pub struct NotarizedBlock {
    pub block: SimplexBlock,
    pub signatures: Vec<(SimplexMessage, Signature, NodeId)>,
}
