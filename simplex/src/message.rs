use crate::block::SimplexBlock;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub content: SimplexBlock,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub content: SimplexBlock,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Timeout {
    pub next_iter: u32,
    pub last_notarized_iter: u32,
    pub last_notarized_block: SimplexBlock,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Finalize {
    pub iter: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct View {
    pub last_notarized_block: SimplexBlock,
}


#[derive(serde::Serialize,serde::Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum SimplexMessage {
    Propose(Propose),
    Vote(Vote),
    Timeout(Timeout),
    Finalize(Finalize),
    View(View),
}
