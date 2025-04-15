use serde_json::to_string;
use crate::block::{HashedSimplexBlock, NotarizedBlock, SimplexBlock, ViewBlock};

pub trait SimplexMessage: Clone + Send + Sync + serde::Serialize + for<'de> serde::Deserialize<'de> {
    fn is_vote(&self) -> bool;
    fn get_sample(&self) -> Option<Vec<u32>>;
    fn get_signature_bytes(&self) -> Option<&[u8]>;
    fn get_vote_header_bytes(&self) -> Option<Vec<u8>>;
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub content: SimplexBlock,
}

#[derive(Eq, Hash, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub iteration: u32,
    pub header: HashedSimplexBlock,
    pub signature: Vec<u8>,
}

#[derive(Eq, Hash, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct ProbVote {
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
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct ProbFinalize {
    pub iter: u32,
    pub sample: Vec<u32>,
    pub proof: Vec<u8>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct View {
    pub last_notarized: ViewBlock,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Request {
    pub last_notarized_length: u32
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Reply {
    pub blocks: Vec<NotarizedBlock>,
}

#[derive(Clone, serde::Serialize,serde::Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum PracticalSimplexMessage {
    Propose(Propose),
    Vote(Vote),
    Timeout(Timeout),
    Finalize(Finalize),
    View(View),
    Request(Request),
    Reply(Reply),
}

#[derive(Clone, serde::Serialize,serde::Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ProbabilisticSimplexMessage {
    Propose(Propose),
    Vote(ProbVote),
    Timeout(Timeout),
    Finalize(ProbFinalize),
    Request(Request),
    Reply(Reply),
}

impl SimplexMessage for PracticalSimplexMessage {
    fn is_vote(&self) -> bool {
        matches!(self, Self::Vote(_))
    }

    fn get_sample(&self) -> Option<Vec<u32>> {
        None
    }

    fn get_signature_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Vote(v) => Some(v.signature.as_slice()),
            _ => None,
        }
    }

    fn get_vote_header_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Vote(v) => to_string(&v.header).ok().map(|s| s.into_bytes()),
            _ => None,
        }
    }
}

impl SimplexMessage for ProbabilisticSimplexMessage {
    fn is_vote(&self) -> bool {
        matches!(self, Self::Vote(_))
    }

    fn get_sample(&self) -> Option<Vec<u32>> {
        match self {
            Self::Vote(v) => Some(v.sample.clone()),
            Self::Finalize(f) => Some(f.sample.clone()),
            _ => None,
        }
    }

    fn get_signature_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Vote(v) => Some(v.signature.as_slice()),
            _ => None,
        }
    }

    fn get_vote_header_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Vote(v) => to_string(&v.header).ok().map(|s| s.into_bytes()),
            _ => None,
        }
    }
}
