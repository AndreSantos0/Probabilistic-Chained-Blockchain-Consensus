use crate::block::{HashedSimplexBlock, NotarizedBlock, SimplexBlock};

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub content: SimplexBlock,
}

#[derive(Eq, Hash, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub iteration: u32,
    pub header: HashedSimplexBlock,
    pub signature: Vec<u8>,
    pub sample: Vec<u32>,
    pub proof: Vec<u8>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Timeout {
    pub next_iter: u32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Finalize {
    pub iter: u32,
    pub sample: Vec<u32>,
    pub proof: Vec<u8>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Request {
    pub last_notarized_length: u32,
    pub curr_iteration: u32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Reply {
    pub blocks: Vec<NotarizedBlock>,
}


#[derive(Clone, serde::Serialize,serde::Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum SimplexMessage {
    Propose(Propose),
    Vote(Vote),
    Timeout(Timeout),
    Finalize(Finalize),
    Request(Request),
    Reply(Reply),
}
