use crate::domain::block::Block;


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
