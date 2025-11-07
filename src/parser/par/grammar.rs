use pest::{Parser, error::Error, iterators::Pairs};
use pest_derive::Parser;

use super::items::{Graph, Node};

#[derive(Parser)]
#[grammar = "../grammar/par.pest"]
pub struct ParParser;

#[allow(clippy::result_large_err)] // We can't do much about it
pub fn parse(file: impl AsRef<str>) -> Result<Graph, Error<Rule>> {
    let rule = ParParser::parse(Rule::Program, file.as_ref())?
        .next()
        .unwrap(); // Unwraps the Rule::Program, never fails

    let mut nodes = vec![];

    let node_list = rule.into_inner(); // Here we have the `Vec<Node | EOI>;

    parse_node_list(node_list, &mut nodes);

    Ok(Graph::new(nodes))
}

pub fn parse_node_list(node_list: Pairs<Rule>, nodes: &mut Vec<Node>) {
    // the node_list now have all the statements.
    // We need to iterate over them and parse them accordingly.
    for node in node_list {
        println!("parse_node_list: {node:#?}");
        match node.as_rule() {
            Rule::Id => {
                let inline_node = parse_id(&node);
                nodes.push(Node::Atomic(inline_node));
            }
            Rule::Inline => {
                let inline_node = parse_inline(node);
                nodes.push(inline_node);
            }
            Rule::ParBlock => {
                let par_node = parse_par_block(node);
                nodes.push(par_node);
            }
            Rule::SeqBlock => {
                let seq_node = parse_seq_block(node);
                nodes.push(seq_node);
            }
            Rule::EOI => break,
            _ => {}
        }
    }
}

fn parse_id(node: &pest::iterators::Pair<Rule>) -> String {
    node.as_str().to_string()
}

fn parse_inline(node: pest::iterators::Pair<Rule>) -> Node {
    let node = node.into_inner().next().unwrap();
    let id = parse_id(&node);
    Node::Atomic(id)
}

fn parse_par_block(node: pest::iterators::Pair<Rule>) -> Node {
    let mut inner_nodes = vec![];
    for inner in node.into_inner() {
        println!("parse_par_block inner: {inner:#?}");
        match inner.as_rule() {
            Rule::Id => {
                let inline_node = parse_id(&inner);
                inner_nodes.push(Node::Atomic(inline_node));
            }
            Rule::Inline => {
                let inline_node = parse_inline(inner);
                inner_nodes.push(inline_node);
            }
            Rule::SeqBlock => {
                let seq_node = parse_seq_block(inner);
                inner_nodes.push(seq_node);
            }
            _ => {}
        }
    }
    Node::Par(inner_nodes)
}

fn parse_seq_block(node: pest::iterators::Pair<Rule>) -> Node {
    let mut inner_nodes = vec![];
    for inner in node.into_inner() {
        match inner.as_rule() {
            Rule::Id => {
                let inline_node = parse_id(&inner);
                inner_nodes.push(Node::Atomic(inline_node));
            }
            Rule::Inline => {
                let inline_node = parse_inline(inner);
                inner_nodes.push(inline_node);
            }
            Rule::ParBlock => {
                let par_node = parse_par_block(inner);
                inner_nodes.push(par_node);
            }
            _ => {}
        }
    }
    Node::Seq(inner_nodes)
}
// - Program
//   - Inline > Id: "a"
//   - ParBlock
//     - SeqBlock
//       - Inline > Id: "b"
//       - Inline > Id: "f"
//     - Inline > Id: "c"
//   - EOI: ""
