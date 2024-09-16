use crate::domain::transaction::Transaction;

pub struct Block {
    pub hash: Vec<u8>,
    pub epoch: i32,
    pub length: i32,
    pub transactions: Vec<Transaction>,
}
