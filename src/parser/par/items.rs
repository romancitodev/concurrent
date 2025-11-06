use crate::parser::items;

/// Represents the program
///
/// It contains a vector of [`Node`]
#[derive(Debug)]
pub struct Graph(pub Vec<Node>);

impl Graph {
    pub fn new(nodes: Vec<Node>) -> Self {
        Self(nodes)
    }

    pub fn to_ir(&self) -> items::Graph {
        let ir_nodes = self.0.iter().map(|node| self.node_to_ir(node)).collect();
        items::Graph::new(ir_nodes)
    }

    fn node_to_ir(&self, node: &Node) -> items::Node {
        match node {
            Node::Par(children) => {
                let ir_children = children
                    .iter()
                    .map(|child| self.node_to_ir(child))
                    .collect();
                items::Node::Par(ir_children)
            }
            Node::Seq(children) => {
                let ir_children = children
                    .iter()
                    .map(|child| self.node_to_ir(child))
                    .collect();
                items::Node::Seq(ir_children)
            }
            Node::Atomic(name) => items::Node::Atomic(name.clone(), vec![], false),
        }
    }
}

/// Represents a Node
/// It can represent a Parallel Node
/// a Sequential Node
/// or a Atomic Node (the atomic operation itself)
#[derive(Debug)]
pub enum Node {
    Par(Vec<Node>),
    Seq(Vec<Node>),
    /// It contains the name of the node.
    Atomic(String),
}
