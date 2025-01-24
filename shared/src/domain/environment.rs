use crate::domain::node::Node;


#[derive(Debug)]
pub struct Environment {
    pub my_node: Node,
    pub nodes: Vec<Node>,
    pub test_flag: bool,
}
