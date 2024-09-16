use crate::domain::block::Block;

pub trait Message {
    fn sender(&self) -> i32;
}

macro_rules! impl_message_for {
    ($t:ty) => {
        impl Message for $t {
            fn sender(&self) -> i32 {
                self.sender
            }
        }
    };
}

pub struct Propose {
    pub content: Block,
    pub sender: i32,
}

pub struct Vote {
    pub content: Block,
    pub sender: i32,
}

impl_message_for!(Propose);
impl_message_for!(Vote);
