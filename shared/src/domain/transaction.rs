use std::hash::{Hash};

#[derive(Hash, Eq, PartialEq, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Transaction {
    pub sender: u32,
    pub id: u32,
    pub amount: Amount,
    pub padding: String,
}

#[derive(Hash, Eq, PartialEq, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Amount {
    integral: u64,
    fractional: u64
}

impl Amount {
    pub fn new(i: u64, f: u64) -> Amount {
        Amount {
            integral: i,
            fractional: f
        }
    }
}

impl Transaction {
    pub fn new(sender: u32, id: u32, amount: Amount, padding_size: usize) -> Self {
        let padding = "X".repeat(padding_size); // Just repeat 'X' for simplicity
        Transaction {
            sender,
            id,
            amount,
            padding,
        }
    }
}
