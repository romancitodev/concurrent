/// Represents the program
///
/// It contains a vector of [`Node`]
#[derive(Debug)]
pub struct Graph(Vec<Node>);

impl Graph {
    pub fn new(nodes: Vec<Node>) -> Self {
        Self(nodes)
    }
}

/// Represents a Node
/// It can represent a Parallel Node
/// a Sequential Node
/// or a Atomic Node (the atomic operation itself)
///
/// For example:
///
/// ```rs
/// // Given this input: {a, b}
/// // It would be represented as:
/// let nodes = parse("{a, b}");
/// assert_eq!(nodes, Node::Par(vec![Node::Atomic("a", vec![]), Node::Atomic("b", vec![])]));
/// ```
///
/// ```rs
/// // Given this input: [a, b]
/// // It would be represented as:\
/// let nodes = parse("[a, b]");
/// assert_eq!(nodes, Node::Seq(vec![Node::Atomic("a", vec![]), Node::Atomic("b", vec![])]));
/// ```
///
/// ## Example with dependencies
///
/// ```rs
/// let nodes = parse("a#{b}")
/// assert_eq!(nodes, Node::Atomic("a", vec![Node::Dep("b")]));
/// ```
#[derive(Debug)]
pub enum Node {
    Par(Vec<Node>),
    Seq(Vec<Node>),
    /// It contains the name of the node and a list of depedencies, every [`Node`] in the vec it's another [`Node::Dep`].
    Atomic(String, Vec<Node>),
    Dep(String),
}
