use pest::{
    Parser,
    error::Error,
    iterators::{Pair, Pairs},
};

use pest_derive::Parser;

use crate::parser::fk::items::{Graph, Node, Stmt};

#[derive(Parser)]
#[grammar = "../grammar/fk.pest"]
pub struct ForkJoinParser;

pub fn parse(file: impl AsRef<str>) -> Result<Graph, Error<Rule>> {
    let rule = ForkJoinParser::parse(Rule::Program, file.as_ref())?
        .next()
        .unwrap(); // Unwraps the Rule::Program, never fails

    let mut nodes = vec![];

    let node_list = rule.into_inner(); // Here we have the `Vec<Statement | EOI>;

    parse_node_list(node_list, &mut nodes);

    Ok(Graph::new(nodes))
}

pub fn parse_node_list(node_list: Pairs<Rule>, nodes: &mut Vec<Stmt>) {
    // the node_list now have all the statements.
    // We need to iterate over them and parse them accordingly.
    for node in node_list {
        let Some(inner) = node.into_inner().next() else {
            break;
        };
        // println!("parse_node_list: {inner:#?}");
        if inner.as_rule() == Rule::LabeledStatement {
            parse_labeled_statement(inner, nodes);
        } else if inner.as_rule() == Rule::UnlabeledStatement {
            parse_unlabeled_statement(inner, nodes);
        } else {
            break;
        }
    }
}

fn parse_labeled_statement(node: Pair<Rule>, nodes: &mut Vec<Stmt>) {
    // println!("parse_labeled_statement (outer): {node:#?}");

    let mut node = node.into_inner();
    let inner = node.next().unwrap();

    // We know tht the node it's a label.

    let inner_label = inner.into_inner().next().unwrap();
    let label = inner_label.as_str().to_string();

    let unlabeled_node = node.next().unwrap().into_inner().next().unwrap();
    let statement = parse_atomic_unlabebled_statement(unlabeled_node);
    nodes.push(Stmt::new(Some(label), statement));
}

fn parse_unlabeled_statement(node: Pair<Rule>, nodes: &mut Vec<Stmt>) {
    let inner = node.into_inner().next().unwrap();
    // println!("parse_unlabeled_statement: {inner:#?}");
    let statement = parse_atomic_unlabebled_statement(inner);
    nodes.push(Stmt::new(None, statement));
}

fn parse_atomic_unlabebled_statement(node: Pair<Rule>) -> Node {
    // println!("parse_atomic_unlabebled_statement: (node) {node:#?}");
    match node.as_rule() {
        Rule::Task => {
            // node.into_inner() -> Id
            let id = node.into_inner().next().unwrap().as_str().to_string();
            // println!("id => {id}");
            Node::Atomic(id)
        }
        Rule::Fork => {
            let id = node.into_inner().next().unwrap().as_str().to_string();
            Node::Fork(id)
        }
        Rule::Goto => {
            let id = node.into_inner().next().unwrap().as_str().to_string();
            Node::Goto(id)
        }
        Rule::Join => Node::Join,
        _ => unreachable!(),
    }
}

// - Program
//   - Statement > UnlabeledStatement > Task > Id: "a"
//   - Statement > UnlabeledStatement > Fork > Id: "LB"
//   - Statement > UnlabeledStatement > Task > Id: "c"
//   - Statement > LabeledStatement
//     - Label > Id: "LD"
//     - UnlabeledStatement > Join: "join\n"
//   - Statement > UnlabeledStatement > Task > Id: "d"
//   - Statement > LabeledStatement
//     - Label > Id: "LB"
//     - UnlabeledStatement > Task > Id: "b"
//   - Statement > UnlabeledStatement > Goto > Id: "LD"
//   - EOI: ""
