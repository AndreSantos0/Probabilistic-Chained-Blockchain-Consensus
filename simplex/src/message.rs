use shared::domain::block::Block;


#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub iter: u32,
    pub content: Block,
    pub sender: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub iter: u32,
    pub content: Block,
    pub sender: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Timeout {
    pub next_iter: u32,
    pub last_notarized_iter: u32,
    pub last_notarized: Block,
    pub sender: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Finalize {
    pub iter: u32,
    pub sender: u32,
}

#[derive(serde::Serialize,serde::Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum SimplexMessage {
    Propose(Propose),
    Vote(Vote),
    Timeout(Timeout),
    Finalize(Finalize),
}
