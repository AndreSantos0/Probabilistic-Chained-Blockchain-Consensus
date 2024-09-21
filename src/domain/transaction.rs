
#[derive(Clone, serde::Serialize,serde::Deserialize,Debug)]
pub struct Transaction {
    pub sender: u32,
    pub id: u32,
    pub amount: f64,
}

impl Transaction {
    pub fn new(sender: u32, id: u32, amount: f64) -> Transaction {
        Transaction {
            sender,
            id,
            amount,
        }
    }
}
