use std::sync::OnceLock;

use bincode::serialize;
use crate::domain::transaction::{Transaction};

static GLOBAL_TRANSACTION: OnceLock<Transaction> = OnceLock::new();

pub struct TransactionGenerator {
    transactions_per_block: usize,
}

impl TransactionGenerator {

    pub fn new(target_size: usize, transactions_per_block: usize) -> Self {
        GLOBAL_TRANSACTION.get_or_init(|| Self::create_transaction(target_size));
        TransactionGenerator { transactions_per_block }
    }

    fn create_transaction(target_size: usize) -> Transaction {
        let mut padding_size = 0;
        loop {
            let tx = Transaction::new(padding_size);
            let size = serialize(&tx).unwrap().len();
            if size >= target_size {
                return tx;
            }
            padding_size += 1;
        }
    }

    /*
    pub fn poll(&mut self, transactions_per_block: usize) -> Vec<Transaction> {
        (0..transactions_per_block).filter_map(|_| self.pool.pop_front()).collect()
    }
     */

    pub fn generate(&mut self) -> Vec<Transaction> {
        let transaction = GLOBAL_TRANSACTION
            .get()
            .expect("Global transaction should be initialized in TransactionGenerator::new");
        vec![transaction.clone(); self.transactions_per_block]
    }
}
