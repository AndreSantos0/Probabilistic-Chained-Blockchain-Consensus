use shared::domain::transaction::Transaction;


#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct StreamletBlock {
    pub hash: Option<Vec<u8>>,
    pub epoch: u32,
    pub length: u32,
    pub transactions: Vec<Transaction>,
}

impl StreamletBlock {

    pub fn new(hash: Option<Vec<u8>>, epoch: u32, length: u32, transactions: Vec<Transaction>) -> Self {
        StreamletBlock {
            hash,
            epoch,
            length,
            transactions
        }
    }
}
