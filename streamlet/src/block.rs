use shared::domain::transaction::Transaction;


pub type NodeId = u32;
pub type Epoch = u32;

#[derive(Eq, Hash, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct StreamletBlock {
    pub hash: Vec<u8>,
    pub epoch: u32,
    pub length: u32,
    pub transactions: Vec<Transaction>,
}

impl StreamletBlock {

    pub fn new(hash: Vec<u8>, epoch: Epoch, length: u32, transactions: Vec<Transaction>) -> Self {
        StreamletBlock {
            hash,
            epoch,
            length,
            transactions
        }
    }
}
