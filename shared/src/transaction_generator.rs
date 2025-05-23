use crate::domain::transaction::{Amount, Transaction};


const INITIAL_NONCE: u32 = 1;
const DEFAULT_AMOUNT_INTEGRAL: u64 = 50;
const DEFAULT_AMOUNT_FRACTIONAL: u64 = 0;
const NUMBER_OF_TRANSACTIONS: usize = 100;

pub struct TransactionGenerator {
    nonce: u32,
    required_padding: usize,
}

impl TransactionGenerator {
    pub fn new(target_size: usize) -> Self {
        let base_tx = Transaction::new(0, INITIAL_NONCE, Amount::new(DEFAULT_AMOUNT_INTEGRAL, DEFAULT_AMOUNT_FRACTIONAL), 0);
        let base_json = serde_json::to_string(&base_tx).unwrap();
        let current_size = base_json.len();
        let required_padding = if current_size < target_size {
            target_size - current_size
        } else {
            0
        };
        TransactionGenerator {
            nonce: INITIAL_NONCE,
            required_padding,
        }
    }

    pub fn generate(&mut self, sender: u32) -> Vec<Transaction> {
        let mut transactions = Vec::with_capacity(NUMBER_OF_TRANSACTIONS);
        for _ in 0..NUMBER_OF_TRANSACTIONS {
            let final_tx = Transaction::new(sender, self.nonce, Amount::new(DEFAULT_AMOUNT_INTEGRAL, DEFAULT_AMOUNT_FRACTIONAL), self.required_padding);
            transactions.push(final_tx);
            self.nonce += 1;
        }
        transactions
    }
}
