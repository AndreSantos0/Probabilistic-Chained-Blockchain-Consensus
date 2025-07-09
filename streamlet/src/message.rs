use crate::block::StreamletBlock;

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Propose {
    pub content: StreamletBlock,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Vote {
    pub content: StreamletBlock,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub enum BaseMessage {
    Propose(Propose),
    Vote(Vote),
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Echo {
    pub message: BaseMessage,
    pub signature: Vec<u8>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub enum StreamletMessage {
    Base(BaseMessage),
    Echo(Echo),
}

impl StreamletMessage {
    pub fn is_echo(&self) -> bool {
        matches!(self, Self::Echo(_))
    }

    pub fn get_base_message(&self) -> Option<BaseMessage> {
        match self {
            Self::Base(base) => Some(base.clone()),
            _ => None,
        }
    }

    pub fn get_echo_message(&self) -> Option<StreamletMessage> {
        match self {
            Self::Echo(echo) => match echo.message.clone() {
                BaseMessage::Propose(propose) => { Some(StreamletMessage::Base(BaseMessage::Propose(propose))) }
                BaseMessage::Vote(vote) => { Some(StreamletMessage::Base(BaseMessage::Vote(vote))) }
            },
            _ => None,
        }
    }

    pub fn get_echo_signature(&self) -> Option<&Vec<u8>> {
        match self {
            Self::Echo(e) => Some(&e.signature),
            _ => None,
        }
    }
}
