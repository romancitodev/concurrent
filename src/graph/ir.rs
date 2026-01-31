use pest::Parser;
use pest::error::Error;
use pest::iterators::{Pair, Pairs};
use pest_derive::Parser;

use super::{fk, par};

#[derive(Debug)]
pub struct Graph(pub Vec<Node>);

impl Graph {
    pub fn new(nodes: Vec<Node>) -> Self {
        Self(nodes)
    }

    pub fn to_fk(&self) -> fk::Graph {
        fk::Graph::from_ir(self)
    }

    pub fn to_par(&self) -> par::Graph {
        par::Graph::from_ir(self)
    }
}

#[derive(Debug, Clone)]
pub enum Node {
    Par(Vec<Node>),
    Seq(Vec<Node>),
    Atomic(String, Vec<Node>, bool),
    Dep(String),
}

#[derive(Parser)]
#[grammar = "../grammar/lang.pest"]
struct IrParser;

#[allow(clippy::result_large_err)]
pub fn parse(input: impl AsRef<str>) -> Result<Graph, Error<Rule>> {
    let rule = IrParser::parse(Rule::Program, input.as_ref())?
        .next()
        .unwrap();

    let mut nodes = vec![];
    let inner = rule.into_inner().next().unwrap().into_inner();
    parse_nodes(inner, &mut nodes);

    Ok(Graph::new(nodes))
}

fn parse_nodes(pairs: Pairs<Rule>, nodes: &mut Vec<Node>) {
    for pair in pairs {
        let inner = pair.into_inner().next().unwrap();
        match inner.as_rule() {
            Rule::Task => nodes.push(parse_task(inner)),
            Rule::Sequencial => {
                let mut children = vec![];
                parse_nodes(
                    inner.into_inner().next().unwrap().into_inner(),
                    &mut children,
                );
                nodes.push(Node::Seq(children));
            }
            Rule::Parallel => {
                let mut children = vec![];
                parse_nodes(
                    inner.into_inner().next().unwrap().into_inner(),
                    &mut children,
                );
                nodes.push(Node::Par(children));
            }
            _ => unreachable!(),
        }
    }
}

fn parse_task(pair: Pair<Rule>) -> Node {
    let mut inner = pair.into_inner();
    let id = inner.next().unwrap().as_str().to_string();

    let mut deps = vec![];
    let mut terminal = false;

    for rule in inner {
        match rule.as_rule() {
            Rule::Deps => {
                for dep in rule.into_inner() {
                    deps.push(Node::Dep(dep.as_str().to_string()));
                }
            }
            Rule::Terminal => terminal = true,
            _ => {}
        }
    }

    Node::Atomic(id, deps, terminal)
}
