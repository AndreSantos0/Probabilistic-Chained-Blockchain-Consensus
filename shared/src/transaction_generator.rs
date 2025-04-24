use crate::domain::transaction::{Amount, Transaction};


const INITIAL_NONCE: u32 = 1;
const MAX_LIST_SIZE: usize = 5;
const MIN_LIST_SIZE: usize = 1;
const DEFAULT_AMOUNT_INTEGRAL: u64 = 50;
const DEFAULT_AMOUNT_FRACTIONAL: u64 = 0;
const LARGE_PRIME: u32 = 1_000_003;
const NUMBER_OF_TRANSACTIONS: usize = 10;

pub struct TransactionGenerator {
    nonce: u32,
}

impl TransactionGenerator {
    pub fn new() -> Self {
        TransactionGenerator {
            nonce: INITIAL_NONCE,
        }
    }

    pub fn generate(&mut self, sender: u32) -> Vec<Transaction> {
        let mut transactions = Vec::with_capacity(NUMBER_OF_TRANSACTIONS);
        for _ in 0..NUMBER_OF_TRANSACTIONS {
            transactions.push(Transaction::new(sender, self.nonce, Amount::new(DEFAULT_AMOUNT_INTEGRAL, DEFAULT_AMOUNT_FRACTIONAL)));
            self.nonce += 1;
        }
        transactions
    }
}
