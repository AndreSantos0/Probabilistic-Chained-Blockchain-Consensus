use crate::domain::block::Block;
use crate::domain::transaction::Transaction;

pub struct Blockchain;

impl Blockchain {

    pub fn new() -> Blockchain {
        Blockchain
    }

    pub fn print(&self) {

    }
    
    pub fn get_next_block(&self, epoch: u32, transactions: Vec<Transaction>) -> Block {
        Block {
            hash: vec![],
            epoch,
            length: 0,
            transactions,
        }
    }

    pub fn get_longest_chain_length(&self) -> u32 {
        todo!()
    }
}
