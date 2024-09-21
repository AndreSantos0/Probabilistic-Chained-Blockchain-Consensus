use crate::domain::transaction::Transaction;


#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Block {
    pub hash: Vec<u8>,
    pub epoch: u32,
    pub length: u32,
    pub transactions: Vec<Transaction>,
}
