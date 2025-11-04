use pest::{
    Parser,
    iterators::{Pair, Pairs},
};
use pest_derive::Parser;

use crate::parser::items::{Graph, Node};

#[derive(Parser)]
#[grammar = "../grammar/lang.pest"]
pub struct LangParser;

pub fn parse(file: impl AsRef<str>) -> Graph {
    let rule = LangParser::parse(Rule::Program, file.as_ref())
        .expect("unsuccesfull parsing")
        .next()
        .unwrap(); // Unwraps the Rule::Program, never fails

    let mut nodes = vec![];

    let node_list = rule.into_inner().next().unwrap().into_inner();

    parse_node_list(node_list, &mut nodes);

    Graph::new(nodes)
}

pub fn parse_node_list(node_list: Pairs<Rule>, nodes: &mut Vec<Node>) {
    for node in node_list {
        // Esentially, we are iterating over the nodes in the node_list
        // And the only rule that can be here is Rule::Node.
        let inner_rule = node.into_inner().next().unwrap();
        match inner_rule.as_rule() {
            Rule::Task => {
                let (id, deps) = parse_atomic_node(inner_rule);
                nodes.push(Node::Atomic(id, deps));
            }
            Rule::Sequencial => {
                // parse sequencial
                let node_list = inner_rule.into_inner().next().unwrap().into_inner();
                let mut seq_nodes = vec![];
                parse_node_list(node_list, &mut seq_nodes);
                nodes.push(Node::Seq(seq_nodes));
            }
            Rule::Parallel => {
                // parse parallel
                let node_list = inner_rule.into_inner().next().unwrap().into_inner();
                let mut par_nodes = vec![];
                parse_node_list(node_list, &mut par_nodes);
                nodes.push(Node::Par(par_nodes));
            }
            _ => unreachable!(),
        }
    }
}

fn parse_atomic_node(inner_rule: Pair<'_, Rule>) -> (String, Vec<Node>) {
    let mut inner_rules = inner_rule.into_inner();
    let id_rule = inner_rules.next().unwrap();
    let id = id_rule.as_str().to_string();

    let mut deps = vec![];
    if let Some(deps_rule) = inner_rules.next() {
        // There are dependencies
        for dep in deps_rule.into_inner() {
            let dep_id = dep.as_str().to_string();
            deps.push(Node::Dep(dep_id));
        }
    }
    (id, deps)
}

// - Program > NodeList
//   - Node > Task > Id: "a"
//   - Node > Parallel > NodeList
//     - Node > Task
//       - Id: "b"
//       - Deps
//         - Id: "v"
//         - Id: "f"
//     - Node > Sequencial > NodeList
//       - Node > Task > Id: "d"
//       - Node > Task > Id: "h"
//   - Node > Parallel > NodeList
//     - Node > Task > Id: "c"
//     - Node > Task > Id: "e"
//   - Node > Task > Id: "f"
