use crate::block::StreamletBlock;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub content: StreamletBlock,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub content: StreamletBlock,
}

#[derive(serde::Serialize,serde::Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum StreamletMessage {
    Propose(Propose),
    Vote(Vote),
}
