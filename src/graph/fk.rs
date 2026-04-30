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
        println!("{:?}", &ir.0);
        conv.build(&ir.0);
        conv.finalize()
    }
}

struct Branch {
    label: String,
    /// we know that we always have a `Par(Vec<Node>)` or `Seq(Vec<Node>)` node here
    stmts: ir::Node,
    target: String,
}

type Id = String;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ctx {
    Main,     // main path
    Deferred, // on branch path
}

struct IrToFk {
    dependencies: HashMap<Id, Vec<Id>>,
    main_path: Vec<Stmt>,
    branches: Vec<Branch>,
    label_counter: usize,
    join_counter: usize,
}

impl IrToFk {
    fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
            main_path: Vec::new(),
            branches: Vec::new(),
            label_counter: 0,
            join_counter: 1,
        }
    }

    fn new_label(&mut self) -> String {
        let label = format!("L{}", self.label_counter);
        self.label_counter += 1;
        label
    }

    fn finalize(mut self) -> Graph {
        let branches = std::mem::take(&mut self.branches);
        self.main_path.push(Stmt::new(
            None,
            Node::Goto {
                id: "_end".to_string(),
            },
        ));
        self.main_path
            .push(Stmt::new(Some("_end".to_string()), Node::Final));
        for branch in branches {
            self.expand_branch(branch.stmts, branch.target);
        }
        Graph::new(self.main_path)
    }

    fn build(&mut self, nodes: &[ir::Node]) {
        self.fetch_dependencies(nodes);
        self.convert_nodes(nodes, Ctx::Main);
    }

    fn fetch_dependencies(&mut self, nodes: &[ir::Node]) {
        for node in nodes {
            match node {
                ir::Node::Atomic(parent, deps, _) => {
                    if deps.is_empty() {
                        continue;
                    }
                    for dep in deps {
                        assert!(
                            matches!(dep, ir::Node::Dep(_)),
                            "Only Dep nodes are allowed as dependencies"
                        );
                        self.dependencies
                            .entry(parent.clone())
                            .or_default()
                            .push(dep.id());
                    }
                }
                ir::Node::Seq(children) | ir::Node::Par(children) => {
                    self.fetch_dependencies(children);
                }
                ir::Node::Dep(_) => {}
            }
        }
    }

    fn convert_nodes(&mut self, nodes: &[ir::Node], ctx: Ctx) {
        self.convert_nodes_with_label(nodes, ctx, None);
    }

    fn convert_nodes_with_label(
        &mut self,
        nodes: &[ir::Node],
        ctx: Ctx,
        initial_label: Option<String>,
    ) {
        let mut pending_label = initial_label;
        let mut idx = 0;

        while idx < nodes.len() {
            let label = pending_label.take();
            match &nodes[idx] {
                ir::Node::Par(branches) => {
                    let next_node = nodes.get(idx + 1);
                    let target = next_node
                        .map(|n| format!("L{}", n.id()))
                        .unwrap_or_else(|| "_end".to_string());
                    let next_has_deps = next_node.map_or(false, |n| self.node_has_dependencies(n));
                    let join_label = if next_node.is_some() && !next_has_deps {
                        Some(target.clone())
                    } else {
                        None
                    };

                    self.convert_parallel(branches, label, target, join_label);
                }
                _ => {
                    self.convert_node(&nodes[idx], label, ctx.clone());
                }
            }
            idx += 1;
        }
    }

    fn update_counter(&mut self) -> usize {
        let current = self.join_counter;
        self.join_counter += 1;
        current
    }

    fn convert_node(&mut self, node: &ir::Node, label: Option<String>, ctx: Ctx) {
        match node {
            ir::Node::Atomic(name, _, is_terminal) => {
                self.resolve_dependencies(name);
                self.main_path
                    .push(Stmt::new(label.clone(), Node::Atomic { id: name.clone() }));
                // I need to check some way if the node itself it's the last on the branch, so we can avoid writing `fork` and `goto` for it.
                if *is_terminal {
                    self.post_terminal_deps(name, ctx);
                } else {
                    self.post_dependencies(name, ctx);
                }
            }
            ir::Node::Seq(children) => {
                self.convert_nodes_with_label(children, ctx, label);
            }
            ir::Node::Par(branches) => {
                let target = "_end".to_string();
                self.convert_parallel(branches, label, target, None);
            }
            // We now that the only way to have a `Dep` node is as a dependency of an `Atomic` node, and we are already handling that case by recursively converting the dependencies before the atomic node itself.
            ir::Node::Dep(id) => {}
        }
    }

    fn post_dependencies(&mut self, parent: &String, ctx: Ctx) {
        if ctx != Ctx::Main {
            return;
        }
        self.dependencies
            .iter()
            .filter(|(_, v)| v.contains(parent))
            .for_each(|(k, _)| {
                self.main_path.push(Stmt::new(
                    None,
                    Node::Goto {
                        id: format!("L{k}"),
                    },
                ))
            });
    }

    fn post_terminal_deps(&mut self, parent: &String, ctx: Ctx) {
        if ctx != Ctx::Main {
            return;
        }
        let mut dependencies = self
            .dependencies
            .iter()
            .filter(|(_, v)| v.contains(parent))
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        dependencies.sort();

        if dependencies.is_empty() {
            return;
        }

        let last = dependencies
            .pop()
            .expect("terminal node should have dependencies");
        for dep in dependencies {
            self.main_path.push(Stmt::new(
                None,
                Node::Fork {
                    id: format!("L{dep}"),
                },
            ));
        }

        self.main_path.push(Stmt::new(
            None,
            Node::Goto {
                id: format!("L{last}"),
            },
        ));
    }

    /// parent: L{parent}
    /// counter: c{counter}
    /// deps: Vec<Node>
    fn resolve_dependencies(&mut self, parent: &String) {
        let id = format!("L{parent}");
        if let Some(deps) = self.dependencies.get(parent)
            && !deps.is_empty()
        {
            let counter = format!("c{}", self.update_counter());
            self.main_path.push(Stmt::new(
                Some(id.clone()),
                Node::Join {
                    id: counter.clone(),
                },
            ));
        }
    }

    /// branches is the list of branches that we need to convert in parallel.
    fn convert_parallel(
        &mut self,
        branches: &[ir::Node],
        main_label: Option<String>,
        target: String,
        join_label: Option<String>,
    ) {
        if branches.is_empty() {
            return;
        }

        let forks = branches
            .iter()
            .skip(1)
            .map(|n| format!("L{}", n.id()))
            .collect::<Vec<_>>();

        // After doing the `deferred` branch, we need to "map" every fork into the main path.
        // Example:
        // $a,{[b,c],[d,e]},f$ then:
        // begin
        //  a
        //  fork L{unknown} <--------- We are here
        //  b
        //  c
        //  LF: join c1
        //  f
        //  goto end
        //  L{unknown}: d
        //              e
        //              goto LF
        //
        // end
        for fork in forks {
            self.main_path
                .push(Stmt::new(None, Node::Fork { id: fork }));
        }

        // We are going to take the first branch as the main. (the most-left branch will be the "main" path always).
        let main_branch = &branches[0];
        let mut main_label = main_label;
        if main_label.is_some() && self.node_has_dependencies(main_branch) {
            main_label = None;
        }
        self.convert_node(main_branch, main_label, Ctx::Deferred);

        if let Some(join_label) = join_label {
            let counter = format!("c{}", self.update_counter());
            self.main_path
                .push(Stmt::new(Some(join_label), Node::Join { id: counter }));
        }

        for branch in &branches[1..] {
            let label = self.new_label();
            self.branches.push(Branch {
                label,                 // L{unknown}
                stmts: branch.clone(), // the entire node.
                target: target.clone(),
            });
        }
    }

    fn expand_branch(&mut self, branch: ir::Node, target: String) {
        match branch {
            ir::Node::Atomic(label, _, is_terminal) => {
                let labeled = format!("L{label}");
                self.main_path
                    .push(Stmt::new(Some(labeled), Node::Atomic { id: label.clone() }));
                let continue_to_target =
                    self.emit_branch_dependencies(&label, &target, is_terminal);
                if continue_to_target {
                    self.main_path
                        .push(Stmt::new(None, Node::Goto { id: target }));
                }
            }
            ir::Node::Par(branch) | ir::Node::Seq(branch) => {
                let first_node = branch
                    .first()
                    .expect("Branch should have at least one node");
                let label = first_node.id();
                let labeled = format!("L{label}");
                self.convert_node(first_node, Some(labeled), Ctx::Deferred);
                let mut continue_to_target = self.emit_branch_dependencies(
                    &label,
                    &target,
                    Self::is_terminal_node(first_node),
                );
                if !continue_to_target {
                    return;
                }
                for node in &branch[1..] {
                    // In case we find a dependency of the current node, we resolve it instead of doing a fork, because the dependency will be already resolved in the main path.
                    // example:
                    // $a,{[b,c#{d}],[d,e]},f$ then:
                    // begin
                    //  a
                    //  fork LD
                    //  b
                    //  LC: c <---- now c have a label.
                    //  LF: join c1
                    //  f
                    //  goto end
                    //  LD: d
                    //      fork LC <---- now d have a dependency on c, so instead of doing a goto, we do a fork to the label of c.
                    //      e
                    //      goto LF
                    //
                    // end
                    // node.
                    self.convert_node(node, None, Ctx::Deferred);
                    let node_id = node.id();
                    continue_to_target = self.emit_branch_dependencies(
                        &node_id,
                        &target,
                        Self::is_terminal_node(node),
                    );
                    if !continue_to_target {
                        return;
                    }
                }
                if continue_to_target {
                    self.main_path
                        .push(Stmt::new(None, Node::Goto { id: target }));
                }
            }
            _ => unreachable!(),
        }
    }

    fn emit_branch_dependencies(&mut self, node_id: &str, target: &str, is_terminal: bool) -> bool {
        let mut dependencies = self
            .dependencies
            .iter()
            .filter(|(_, v)| v.iter().any(|dep| dep == node_id))
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        dependencies.sort();

        if dependencies.is_empty() {
            return !is_terminal;
        }

        if is_terminal {
            let last = dependencies
                .pop()
                .expect("terminal node should have dependencies");

            for dep in dependencies {
                let label = format!("L{dep}");
                self.main_path
                    .push(Stmt::new(None, Node::Fork { id: label }));
            }

            let label = format!("L{last}");
            self.main_path
                .push(Stmt::new(None, Node::Goto { id: label }));
            return false;
        }

        for dep in dependencies {
            let label = format!("L{dep}");
            if label == target {
                continue;
            }
            self.main_path
                .push(Stmt::new(None, Node::Fork { id: label }));
        }

        true
    }

    fn node_has_dependencies(&self, node: &ir::Node) -> bool {
        self.dependencies
            .get(&node.id())
            .map(|deps| !deps.is_empty())
            .unwrap_or(false)
    }

    fn is_terminal_node(node: &ir::Node) -> bool {
        matches!(node, ir::Node::Atomic(_, _, true))
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
    Final,
    Join { id: String },
    Goto { id: String },
    Fork { id: String },
    Atomic { id: String },
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
            Node::Atomic { id }
        }
        Rule::Fork => {
            let id = pair.into_inner().next().unwrap().as_str().to_string();
            Node::Fork { id }
        }
        Rule::Goto => {
            let id = pair.into_inner().next().unwrap().as_str().to_string();
            Node::Goto { id }
        }
        Rule::Join => {
            let id = pair.into_inner().next().unwrap().as_str().to_string();
            Node::Join { id }
        }
        _ => unreachable!(),
    }
}
