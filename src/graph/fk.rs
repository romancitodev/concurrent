use std::collections::HashMap;

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
        conv.collect_dependencies(&ir.0);
        conv.convert_nodes(&ir.0, false);
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
    dependencies: HashMap<String, Vec<(String, String)>>,
    node_joins: HashMap<String, (String, String)>,
    join_counter: usize,
}

impl IrToFk {
    fn new() -> Self {
        Self {
            main_stmts: Vec::new(),
            deferred_branches: Vec::new(),
            label_counter: 0,
            dependencies: HashMap::new(),
            node_joins: HashMap::new(),
            join_counter: 0,
        }
    }

    fn new_label(&mut self) -> String {
        let label = format!("L{}", self.label_counter);
        self.label_counter += 1;
        label
    }

    fn finalize(mut self) -> Graph {
        let mut result = std::mem::take(&mut self.main_stmts);

        if let Some(last) = result.last() {
            if !matches!(last.node, Node::Goto(_)) {
                result.push(Stmt::new(None, Node::Goto("_end".to_string())));
            }
        }

        self.append_deferred_branches(&mut result);

        result.push(Stmt::new(
            Some("_end".to_string()),
            Node::Atomic("end".to_string()),
        ));

        let optimized = Self::optimize_stmts(result);

        Graph::new(optimized)
    }

    fn append_deferred_branches(&mut self, result: &mut Vec<Stmt>) {
        for branch in self.deferred_branches.drain(..) {
            if let Some(first) = branch.stmts.first() {
                result.push(Stmt::new(Some(branch.label), first.node.clone()));
                let mut last_is_goto = matches!(first.node, Node::Goto(_));
                for stmt in branch.stmts.into_iter().skip(1) {
                    last_is_goto = matches!(stmt.node, Node::Goto(_));
                    result.push(stmt);
                }
                if !last_is_goto {
                    result.push(Stmt::new(None, Node::Goto(branch.goto_target)));
                }
            }
        }
    }

    fn optimize_stmts(stmts: Vec<Stmt>) -> Vec<Stmt> {
        let mut used_labels = std::collections::HashSet::new();
        for stmt in &stmts {
            match &stmt.node {
                Node::Goto(lbl) | Node::Fork(lbl) => {
                    used_labels.insert(lbl.clone());
                }
                _ => {}
            }
        }

        let mut optimized: Vec<Stmt> = Vec::new();
        for mut stmt in stmts {
            if let Some(label) = &stmt.label {
                if !used_labels.contains(label) {
                    stmt.label = None;
                }
            }

            if stmt.label.is_none() && matches!(stmt.node, Node::Join(_)) {
                continue;
            }

            if let Some(last) = optimized.last() {
                if matches!(last.node, Node::Goto(_)) && stmt.label.is_none() {
                    continue;
                }
            }

            if let Some(last) = optimized.last() {
                if last.label.is_none() {
                    if let Node::Fork(ref target) | Node::Goto(ref target) = last.node {
                        if stmt.label.as_deref() == Some(target) {
                            optimized.pop();
                        }
                    }
                }
            }
            optimized.push(stmt);
        }
        optimized
    }

    fn collect_dependencies(&mut self, nodes: &[ir::Node]) {
        for node in nodes {
            match node {
                ir::Node::Atomic(name, deps, _) => {
                    self.collect_atomic_deps(name, deps);
                }
                ir::Node::Seq(children) | ir::Node::Par(children) => {
                    self.collect_dependencies(children);
                }
                ir::Node::Dep(_) => {}
            }
        }
    }

    fn collect_atomic_deps(&mut self, name: &str, deps: &[ir::Node]) {
        let mut has_deps = false;
        for dep in deps {
            if let ir::Node::Dep(_) = dep {
                has_deps = true;
                break;
            }
        }
        if has_deps {
            let counter = format!("c{}", self.join_counter + 1);
            self.join_counter += 1;
            let label = self.new_label();
            self.node_joins
                .insert(name.to_string(), (label.clone(), counter.clone()));
            for dep in deps {
                if let ir::Node::Dep(dep_name) = dep {
                    self.dependencies
                        .entry(dep_name.clone())
                        .or_default()
                        .push((label.clone(), counter.clone()));
                }
            }
        }
        for dep in deps {
            if !matches!(dep, ir::Node::Dep(_)) {
                self.collect_dependencies(&[dep.clone()]);
            }
        }
    }

    fn convert_nodes(&mut self, nodes: &[ir::Node], has_next_parent: bool) {
        let len = nodes.len();
        for (i, node) in nodes.iter().enumerate() {
            let has_next = if i + 1 < len { true } else { has_next_parent };
            self.convert_node(node, has_next);
        }
    }

    fn convert_node(&mut self, node: &ir::Node, has_next: bool) {
        match node {
            ir::Node::Atomic(name, deps, _) => {
                self.convert_atomic(name, deps, has_next);
            }
            ir::Node::Seq(children) => {
                self.convert_nodes(children, has_next);
            }
            ir::Node::Par(branches) => {
                self.convert_parallel(branches, has_next);
            }
            ir::Node::Dep(_) => {}
        }
    }

    fn convert_atomic(&mut self, name: &str, deps: &[ir::Node], has_next: bool) {
        for dep in deps {
            if !matches!(dep, ir::Node::Dep(_)) {
                self.convert_node(dep, false);
            }
        }

        if let Some((label, counter)) = self.node_joins.get(name) {
            self.main_stmts
                .push(Stmt::new(Some(label.clone()), Node::Join(counter.clone())));
        }

        self.main_stmts
            .push(Stmt::new(None, Node::Atomic(name.to_string())));

        if let Some(deps) = self.dependencies.get(name) {
            let len = deps.len();
            for (i, (label, _)) in deps.iter().enumerate() {
                if has_next || i < len - 1 {
                    self.main_stmts
                        .push(Stmt::new(None, Node::Fork(label.clone())));
                } else {
                    self.main_stmts
                        .push(Stmt::new(None, Node::Goto(label.clone())));
                }
            }
        }
    }

    fn convert_parallel(&mut self, branches: &[ir::Node], has_next: bool) {
        if branches.is_empty() {
            return;
        }

        if branches.len() == 1 {
            self.convert_node(&branches[0], has_next);
            return;
        }

        let labels = branches
            .iter()
            .map(|_| self.new_label())
            .collect::<Vec<_>>();

        let join_label = self.new_label();
        let join_counter = format!("c{}", self.join_counter + 1);
        self.join_counter += 1;

        for label in labels.iter().skip(1) {
            self.main_stmts
                .push(Stmt::new(None, Node::Fork(label.clone())))
        }

        self.convert_node(&branches[0], false);

        let mut needs_goto = true;
        if let Some(last) = self.main_stmts.last() {
            if matches!(last.node, Node::Goto(_)) {
                needs_goto = false;
            }
        }
        if needs_goto {
            self.main_stmts
                .push(Stmt::new(None, Node::Goto(join_label.clone())));
        }

        for (branch, label) in branches.iter().zip(labels.iter()).skip(1) {
            let mut sub = IrToFk::new();
            sub.label_counter = self.label_counter;
            sub.join_counter = self.join_counter;
            for (k, v) in &self.dependencies {
                sub.dependencies.insert(k.clone(), v.clone());
            }
            for (k, v) in &self.node_joins {
                sub.node_joins.insert(k.clone(), v.clone());
            }
            sub.convert_node(branch, false);

            self.label_counter = sub.label_counter;
            self.join_counter = sub.join_counter;
            for (k, v) in &sub.dependencies {
                self.dependencies.insert(k.clone(), v.clone());
            }
            self.deferred_branches.push(Branch {
                label: label.clone(),
                stmts: sub.main_stmts,
                goto_target: join_label.clone(),
            });

            self.deferred_branches.extend(sub.deferred_branches);
        }

        self.main_stmts
            .push(Stmt::new(Some(join_label), Node::Join(join_counter)));
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
    Join(String),
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
            let id = pair.into_inner().next().unwrap().as_str().to_string();
            Node::Join(id)
        }
        _ => unreachable!(),
    }
}
