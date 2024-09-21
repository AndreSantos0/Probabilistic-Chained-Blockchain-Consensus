use rand::Rng;
use crate::domain::transaction::Transaction;


const INITIAL_NONCE: u32 = 1;
const MAX_LIST_SIZE: usize = 5;
const MIN_LIST_SIZE: usize = 1;
const DEFAULT_AMOUNT: f64 = 50.0;
const LARGE_PRIME: u32 = 1_000_003;

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
        let mut rng = rand::thread_rng();
        let list_size = rng.gen_range(MIN_LIST_SIZE..MAX_LIST_SIZE);

        let mut transactions = Vec::with_capacity(list_size);
        for _ in 0..list_size {
            let id = sender * LARGE_PRIME + self.nonce;
            self.nonce += 1;
            transactions.push(Transaction::new(sender, id, DEFAULT_AMOUNT));
        }

        transactions
    }
}
