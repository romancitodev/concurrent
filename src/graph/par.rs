use pest::Parser;
use pest::error::Error;
use pest::iterators::Pairs;
use pest_derive::Parser;

use super::ir;

#[derive(Debug)]
pub struct Graph(pub Vec<Node>);

impl Graph {
    pub fn new(nodes: Vec<Node>) -> Self {
        Self(nodes)
    }

    pub fn to_ir(&self) -> ir::Graph {
        ir::Graph::new(self.0.iter().map(node_to_ir).collect())
    }

    pub fn from_ir(ir: &ir::Graph) -> Self {
        Graph::new(ir.0.iter().map(ir_to_node).collect())
    }
}

#[derive(Debug)]
pub enum Node {
    Par(Vec<Node>),
    Seq(Vec<Node>),
    Atomic(String),
}

fn node_to_ir(node: &Node) -> ir::Node {
    match node {
        Node::Par(children) => ir::Node::Par(children.iter().map(node_to_ir).collect()),
        Node::Seq(children) => ir::Node::Seq(children.iter().map(node_to_ir).collect()),
        Node::Atomic(name) => ir::Node::Atomic(name.clone(), vec![], false),
    }
}

fn ir_to_node(node: &ir::Node) -> Node {
    match node {
        ir::Node::Par(children) => Node::Par(children.iter().map(ir_to_node).collect()),
        ir::Node::Seq(children) => Node::Seq(children.iter().map(ir_to_node).collect()),
        ir::Node::Atomic(name, deps, _) => {
            assert!(deps.is_empty(), "Par cannot represent dependencies");
            Node::Atomic(name.clone())
        }
        ir::Node::Dep(_) => panic!("Par cannot represent dependencies"),
    }
}

#[derive(Parser)]
#[grammar = "../grammar/par.pest"]
struct ParParser;

#[allow(clippy::result_large_err)]
pub fn parse(input: impl AsRef<str>) -> Result<Graph, Error<Rule>> {
    let rule = ParParser::parse(Rule::Program, input.as_ref())?
        .next()
        .unwrap();

    let mut nodes = vec![];
    parse_nodes(rule.into_inner(), &mut nodes);

    Ok(Graph::new(nodes))
}

fn parse_nodes(pairs: Pairs<Rule>, nodes: &mut Vec<Node>) {
    for pair in pairs {
        match pair.as_rule() {
            Rule::Id => nodes.push(Node::Atomic(pair.as_str().to_string())),
            Rule::Inline => {
                let id = pair.into_inner().next().unwrap().as_str().to_string();
                nodes.push(Node::Atomic(id));
            }
            Rule::ParBlock => nodes.push(parse_par_block(pair)),
            Rule::SeqBlock => nodes.push(parse_seq_block(pair)),
            Rule::EOI => break,
            _ => {}
        }
    }
}

fn parse_par_block(pair: pest::iterators::Pair<Rule>) -> Node {
    let mut children = vec![];

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::Id => children.push(Node::Atomic(inner.as_str().to_string())),
            Rule::Inline => {
                let id = inner.into_inner().next().unwrap().as_str().to_string();
                children.push(Node::Atomic(id));
            }
            Rule::SeqBlock => children.push(parse_seq_block(inner)),
            _ => {}
        }
    }

    Node::Par(children)
}

fn parse_seq_block(pair: pest::iterators::Pair<Rule>) -> Node {
    let mut children = vec![];

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::Id => children.push(Node::Atomic(inner.as_str().to_string())),
            Rule::Inline => {
                let id = inner.into_inner().next().unwrap().as_str().to_string();
                children.push(Node::Atomic(id));
            }
            Rule::ParBlock => children.push(parse_par_block(inner)),
            _ => {}
        }
    }

    Node::Seq(children)
}
