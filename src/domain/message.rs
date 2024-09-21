use crate::domain::block::Block;

pub trait MessageTrait: Send + Sync {
    fn sender(&self) -> u32;
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub content: Block,
    pub sender: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub content: Block,
    pub sender: u32,
}

#[derive(serde::Serialize,serde::Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    Propose(Propose),
    Vote(Vote),
}

impl MessageTrait for Message {
    fn sender(&self) -> u32 {
        match self {
            Message::Propose(p) => p.sender,
            Message::Vote(v) => v.sender,
        }
    }
}
