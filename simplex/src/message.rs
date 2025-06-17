use bincode::serialize;
use crate::block::{SimplexBlockHeader, NotarizedBlock, SimplexBlock, VoteSignature, Iteration, NodeId};

pub trait SimplexMessage: Clone + Send + Sync + serde::Serialize + for<'de> serde::Deserialize<'de> {
    fn is_vote(&self) -> bool;
    fn get_signature_bytes(&self) -> Option<&[u8]>;
    fn get_vote_header_bytes(&self) -> Option<Vec<u8>>;
    fn get_sample_set(&self) -> Option<&Vec<u32>>;
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub content: SimplexBlock,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct ProbPropose {
    pub content: SimplexBlock,
    pub last_notarized_iter: Iteration,
    pub last_notarized_cert: Vec<VoteSignature>,
}

#[derive(Eq, Hash, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub iteration: Iteration,
    pub header: SimplexBlockHeader,
    pub signature: Vec<u8>,
}

#[derive(Eq, Hash, PartialEq, Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct ProbVote {
    pub iteration: Iteration,
    pub header: SimplexBlockHeader,
    pub signature: Vec<u8>,
    pub sample: Vec<u32>,
    pub proof: Vec<u8>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Timeout {
    pub next_iter: Iteration,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Finalize {
    pub iter: Iteration,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct ProbFinalize {
    pub iter: Iteration,
    pub sample: Vec<u32>,
    pub proof: Vec<u8>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct View {
    pub last_notarized_block_header: SimplexBlockHeader,
    pub last_notarized_block_cert: Vec<VoteSignature>,
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
pub enum ProbabilisticSimplexMessage {
    Propose(ProbPropose),
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

    fn get_signature_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Vote(v) => Some(v.signature.as_slice()),
            _ => None,
        }
    }

    fn get_vote_header_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Vote(v) => serialize(&v.header).ok(),
            _ => None,
        }
    }

    fn get_sample_set(&self) -> Option<&Vec<u32>> {
        None
    }
}

impl SimplexMessage for ProbabilisticSimplexMessage {

    fn is_vote(&self) -> bool {
        matches!(self, Self::Vote(_))
    }

    fn get_signature_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Vote(v) => Some(v.signature.as_slice()),
            _ => None,
        }
    }

    fn get_vote_header_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Vote(v) => serialize(&v.header).ok(),
            _ => None,
        }
    }

    fn get_sample_set(&self) -> Option<&Vec<u32>> {
        match self {
            Self::Vote(v) => Some(&v.sample),
            Self::Finalize(v) => Some(&v.sample),
            _ => None,
        }
    }
}


pub enum Dispatch {
    Propose(SimplexBlock),
    ProbPropose(SimplexBlock, Iteration, Vec<VoteSignature>),
    Vote(Iteration, SimplexBlockHeader),
    Timeout(Iteration),
    Finalize(Iteration),
    View(SimplexBlockHeader, Vec<VoteSignature>),
    Request(u32, NodeId),
    Reply(Vec<NotarizedBlock>, NodeId),
}
