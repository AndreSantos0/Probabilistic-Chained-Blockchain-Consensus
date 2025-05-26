use bincode::serialize;
use crate::domain::transaction::{Transaction};


const NUMBER_OF_TRANSACTIONS: usize = 1000;

pub struct TransactionGenerator {
    required_padding: usize,
}

impl TransactionGenerator {
    pub fn new(target_size: usize) -> Self {
        let mut padding_size = 0;
        loop {
            let tx = Transaction::new(padding_size);
            let size = serialize(&tx).unwrap().len();
            if size >= target_size {
                break TransactionGenerator {
                    required_padding: padding_size,
                };
            }
            padding_size += 1;
        }
    }

    pub fn generate(&mut self) -> Vec<Transaction> {
        let mut transactions = Vec::with_capacity(NUMBER_OF_TRANSACTIONS);
        for _ in 0..NUMBER_OF_TRANSACTIONS {
            let final_tx = Transaction::new(self.required_padding);
            transactions.push(final_tx);
        }
        transactions
    }
}
