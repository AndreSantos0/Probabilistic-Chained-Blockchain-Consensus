use bincode::serialize;
use crate::domain::transaction::{Transaction};


pub struct TransactionGenerator {
    required_padding: usize,
    n_transactions: usize,
}

impl TransactionGenerator {
    pub fn new(target_size: usize, n_transactions: usize) -> Self {
        let mut padding_size = 0;
        loop {
            let tx = Transaction::new(padding_size);
            let size = serialize(&tx).unwrap().len();
            if size >= target_size {
                break TransactionGenerator {
                    required_padding: padding_size,
                    n_transactions

                };
            }
            padding_size += 1;
        }
    }

    pub fn generate(&self) -> Vec<Transaction> {
        let tx = Transaction::new(self.required_padding);
        vec![tx; self.n_transactions]
    }
}
