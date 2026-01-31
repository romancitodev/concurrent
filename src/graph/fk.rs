use pest::Parser;
use pest::error::Error;
use pest::iterators::{Pair, Pairs};
use pest_derive::Parser;

use super::cfg::ControlFlowGraph;
use super::ir;

#[derive(Debug)]
pub struct Graph(pub Vec<Stmt>);

impl Graph {
    pub fn new(stmts: Vec<Stmt>) -> Self {
        Self(stmts)
    }

    pub fn to_ir(&self) -> ir::Graph {
        ControlFlowGraph::from_graph(self).to_ir()
    }

    pub fn from_ir(ir: &ir::Graph) -> Self {
        let mut conv = IrToFk::new();
        conv.convert_nodes(&ir.0);
        conv.finalize()
    }
}

struct Branch {
    label: String,
    stmts: Vec<Stmt>,
    goto_target: String,
}

struct IrToFk {
    main_stmts: Vec<Stmt>,
    deferred_branches: Vec<Branch>,
    label_counter: usize,
}

impl IrToFk {
    fn new() -> Self {
        Self {
            main_stmts: Vec::new(),
            deferred_branches: Vec::new(),
            label_counter: 0,
        }
    }

    fn new_label(&mut self) -> String {
        let label = format!("L{}", self.label_counter);
        self.label_counter += 1;
        label
    }

    fn finalize(mut self) -> Graph {
        let mut result = self.main_stmts;
        for branch in self.deferred_branches.drain(..) {
            if let Some(first) = branch.stmts.first() {
                result.push(Stmt::new(Some(branch.label), first.node.clone()));
                for stmt in branch.stmts.into_iter().skip(1) {
                    result.push(stmt);
                }
                result.push(Stmt::new(None, Node::Goto(branch.goto_target)));
            }
        }
        Graph::new(result)
    }

    fn convert_nodes(&mut self, nodes: &[ir::Node]) {
        for node in nodes {
            self.convert_node(node);
        }
    }

    fn convert_node(&mut self, node: &ir::Node) {
        match node {
            ir::Node::Atomic(name, _, is_terminal) => {
                self.main_stmts
                    .push(Stmt::new(None, Node::Atomic(name.clone())));
                if *is_terminal {
                    self.main_stmts
                        .push(Stmt::new(None, Node::Goto("end".to_string())));
                }
            }
            ir::Node::Seq(children) => {
                self.convert_nodes(children);
            }
            ir::Node::Par(branches) => {
                self.convert_parallel(branches);
            }
            ir::Node::Dep(_) => {}
        }
    }

    fn convert_parallel(&mut self, branches: &[ir::Node]) {
        if branches.is_empty() {
            return;
        }
        if branches.len() == 1 {
            self.convert_node(&branches[0]);
            return;
        }

        let join_label = self.new_label();
        let join_counter = format!("c{}", self.label_counter);

        let branch_labels: Vec<String> = branches[1..]
            .iter()
            .map(|branch| format!("L{}", Self::first_node_name(branch)))
            .collect();

        for label in &branch_labels {
            self.main_stmts
                .push(Stmt::new(None, Node::Fork(label.clone())));
        }

        self.convert_node(&branches[0]);

        self.main_stmts.push(Stmt::new(
            Some(join_label.clone()),
            Node::Join(Some(join_counter)),
        ));

        for (i, branch) in branches[1..].iter().enumerate() {
            let mut branch_conv = IrToFk::new();
            branch_conv.label_counter = self.label_counter;
            branch_conv.convert_node(branch);
            self.label_counter = branch_conv.label_counter;

            self.deferred_branches.push(Branch {
                label: branch_labels[i].clone(),
                stmts: branch_conv.main_stmts,
                goto_target: join_label.clone(),
            });

            self.deferred_branches.extend(branch_conv.deferred_branches);
        }
    }

    fn first_node_name(node: &ir::Node) -> String {
        match node {
            ir::Node::Atomic(name, _, _) => name.clone(),
            ir::Node::Seq(children) if !children.is_empty() => Self::first_node_name(&children[0]),
            ir::Node::Par(branches) if !branches.is_empty() => Self::first_node_name(&branches[0]),
            _ => "unknown".to_string(),
        }
    }
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

#[derive(Debug, Clone)]
pub enum Node {
    Join(Option<String>),
    Goto(String),
    Fork(String),
    Atomic(String),
}

#[derive(Parser)]
#[grammar = "../grammar/fk.pest"]
struct FkParser;

#[allow(clippy::result_large_err)]
pub fn parse(input: impl AsRef<str>) -> Result<Graph, Error<Rule>> {
    let rule = FkParser::parse(Rule::Program, input.as_ref())?
        .next()
        .unwrap();

    let mut stmts = vec![];
    parse_statements(rule.into_inner(), &mut stmts);

    Ok(Graph::new(stmts))
}

fn parse_statements(pairs: Pairs<Rule>, stmts: &mut Vec<Stmt>) {
    for pair in pairs {
        let Some(inner) = pair.into_inner().next() else {
            break;
        };

        match inner.as_rule() {
            Rule::LabeledStatement => parse_labeled(inner, stmts),
            Rule::UnlabeledStatement => parse_unlabeled(inner, stmts),
            _ => break,
        }
    }
}

fn parse_labeled(pair: Pair<Rule>, stmts: &mut Vec<Stmt>) {
    let mut inner = pair.into_inner();
    let label_pair = inner.next().unwrap();
    let label = label_pair.into_inner().next().unwrap().as_str().to_string();

    let node_pair = inner.next().unwrap().into_inner().next().unwrap();
    let node = parse_node(node_pair);

    stmts.push(Stmt::new(Some(label), node));
}

fn parse_unlabeled(pair: Pair<Rule>, stmts: &mut Vec<Stmt>) {
    let inner = pair.into_inner().next().unwrap();
    let node = parse_node(inner);
    stmts.push(Stmt::new(None, node));
}

fn parse_node(pair: Pair<Rule>) -> Node {
    match pair.as_rule() {
        Rule::Task => {
            let id = pair.into_inner().next().unwrap().as_str().to_string();
            Node::Atomic(id)
        }
        Rule::Fork => {
            let id = pair.into_inner().next().unwrap().as_str().to_string();
            Node::Fork(id)
        }
        Rule::Goto => {
            let id = pair.into_inner().next().unwrap().as_str().to_string();
            Node::Goto(id)
        }
        Rule::Join => {
            let id = pair.into_inner().next().map(|p| p.as_str().to_string());
            Node::Join(id)
        }
        _ => unreachable!(),
    }
}
