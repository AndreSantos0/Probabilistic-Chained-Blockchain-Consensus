use crate::block::{HashedNotarizedBlock, NotarizedBlock, SimplexBlock};

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub content: SimplexBlock,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub content: SimplexBlock,
    pub signature: Vec<u8>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Timeout {
    pub next_iter: u32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Finalize {
    pub iter: u32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct View {
    pub last_notarized: HashedNotarizedBlock,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Request {
    pub last_notarized_length: u32,
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
    View(View),
    Request(Request),
    Reply(Reply),
}
