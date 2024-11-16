use shared::domain::block::Block;

const GENESIS_EPOCH: u32 = 0;
const GENESIS_LENGTH: u32 = 0;
const N_NODES_FOR_FINALIZATION: usize = 3;
const FINALIZED_BLOCKS_FILENAME: &str = "FinalizedBlocks";


pub struct Blockchain {
    nodes: Vec<Block>,
    my_node_id: u32,
}

impl Blockchain {

    pub fn new(my_node_id: u32) -> Self {
        let mut block_nodes = Vec::new();
        block_nodes.push(Self::genesis_block());
        Blockchain { nodes: block_nodes, my_node_id }
    }

    fn genesis_block() -> Block {
        Block::new(None, GENESIS_EPOCH, GENESIS_LENGTH, Vec::new())
    }

    pub fn print(&self) {

    }
}
