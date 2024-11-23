use shared::connection::{NodeId, Signature};
use shared::domain::transaction::Transaction;
use crate::message::{SimplexMessage};

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct SimplexBlock {
    pub hash: Option<Vec<u8>>,
    pub iteration: u32,
    pub length: u32,
    pub transactions: Vec<Transaction>,
    pub signatures: Vec<(SimplexMessage, NodeId, Signature)>,
}

impl SimplexBlock {

    pub fn new(hash: Option<Vec<u8>>, iteration: u32, length: u32, transactions: Vec<Transaction>) -> Self {
        SimplexBlock {
            hash,
            iteration,
            length,
            transactions,
            signatures: Vec::new(),
        }
    }
}
