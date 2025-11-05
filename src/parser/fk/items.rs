/// Represents the program
///
/// It contains a vector of [`Node`]
#[derive(Debug)]
pub struct Graph(pub Vec<Stmt>);

impl Graph {
    pub fn new(nodes: Vec<Stmt>) -> Self {
        Self(nodes)
    }
}

/// Represents a Node
/// It can represent a join node
/// a fork Node
/// or a Atomic Node (the atomic operation itself)
#[derive(Debug, Clone)]
pub enum Node {
    Join,
    Goto(String),
    Fork(String),
    /// It contains the name of the node
    Atomic(String),
}

#[derive(Debug)]
pub struct Stmt {
    pub label: Option<String>,
    pub node: Node,
}

impl Stmt {
    pub fn new(label: Option<String>, node: Node) -> Self {
        Self { label, node }
    }
}

#[derive(Debug)]
pub struct LabeledStmt {
    pub label: String,
    pub node: Node,
}

impl From<LabeledStmt> for Stmt {
    fn from(labeled: LabeledStmt) -> Self {
        Self {
            label: Some(labeled.label),
            node: labeled.node,
        }
    }
}

impl From<Stmt> for LabeledStmt {
    fn from(value: Stmt) -> Self {
        Self {
            label: value.label.unwrap(),
            node: value.node,
        }
    }
}
