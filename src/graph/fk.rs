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
    Main,         // main path
    Deferred,     // on branch path
    LastDeferred, // last deferred node
}

struct IrToFk {
    dependencies: HashMap<Id, Vec<Id>>,
    main_path: Vec<Stmt>,
    branches: Vec<Branch>,
    label_counter: usize,
}

impl IrToFk {
    fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
            main_path: Vec::new(),
            branches: Vec::new(),
            label_counter: 0,
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
        for node in nodes {
            self.convert_node(node, None, ctx.clone());
        }
    }

    fn update_counter(&mut self) -> usize {
        let current = self.label_counter;
        self.label_counter += 1;
        current
    }

    fn convert_node(&mut self, node: &ir::Node, label: Option<String>, ctx: Ctx) {
        match node {
            ir::Node::Atomic(name, _, _) => {
                self.resolve_dependencies(name);
                self.main_path
                    .push(Stmt::new(label.clone(), Node::Atomic { id: name.clone() }));
                // I need to check some way if the node itself it's the last on the branch, so we can avoid writing `fork` and `goto` for it.
                self.post_dependencies(name, ctx);
            }
            ir::Node::Seq(children) => {
                self.convert_nodes(children, ctx);
            }
            ir::Node::Par(branches) => {
                self.convert_parallel(branches);
            }
            // We now that the only way to have a `Dep` node is as a dependency of an `Atomic` node, and we are already handling that case by recursively converting the dependencies before the atomic node itself.
            ir::Node::Dep(id) => {}
        }
    }

    fn post_dependencies(&mut self, parent: &String, ctx: Ctx) {
        if ctx == Ctx::LastDeferred {
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
    fn convert_parallel(&mut self, branches: &[ir::Node]) {
        if branches.is_empty() {
            return;
        }

        let name = Self::first_node_name(&branches[0]);
        let join = format!("L{name}"); // `self.label_counter` for example.
        let main_branch = &branches[0];
        let dep_join = self.first_dependency_label(main_branch);
        let target = dep_join.clone().unwrap_or_else(|| join.clone());
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
        let last_node = main_branch.last_node();
        self.convert_node(main_branch, None, Ctx::Deferred);

        // FIXME: temporal fix, this works on `parallel.fk` but doesn't work on `terminal.fk`
        // Then, we need to check if the current node has explicit dependencies, then if that is true, just skip the join.
        if dep_join.is_none()
            && let Some(node) = last_node
            && matches!(node, ir::Node::Atomic(_, _, _))
            && self.dependencies.get(&node.id()).is_none()
        {
            let counter = format!("c{}", self.update_counter());
            // let join = format!("L{}", node.id());
            self.main_path
                .push(Stmt::new(Some(join.clone()), Node::Join { id: counter }));
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
            ir::Node::Atomic(label, _, _) => {
                let labeled = format!("L{label}");
                self.main_path
                    .push(Stmt::new(Some(labeled), Node::Atomic { id: label.clone() }));

                // this is what post_dependencies does
                self.dependencies
                    .iter()
                    .filter(|(_, v)| v.contains(&label))
                    .for_each(|(k, _)| {
                        self.main_path
                            .push(Stmt::new(None, Node::Fork { id: k.clone() }));
                    });
                self.main_path
                    .push(Stmt::new(None, Node::Goto { id: target }));
            }
            ir::Node::Par(branch) | ir::Node::Seq(branch) => {
                let first_node = branch
                    .first()
                    .expect("Branch should have at least one node");
                let label = first_node.id();
                let labeled = format!("L{label}");
                self.convert_node(first_node, Some(labeled), Ctx::Deferred);
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
                    let ctx = if node.last_node().is_some_and(|n| n == node) {
                        Ctx::LastDeferred
                    } else {
                        Ctx::Deferred
                    };
                    self.convert_node(node, None, ctx);
                    let mut dependencies = self
                        .dependencies
                        .iter()
                        .filter(|(_, v)| v.contains(&node.id()))
                        .map(|(k, _)| k.clone())
                        .collect::<Vec<_>>();

                    if dependencies.is_empty() {
                        continue;
                    }

                    let last = dependencies
                        .pop()
                        .expect("dependencies should not be empty");

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
                            id: format!("L{}", last.clone()),
                        },
                    ));
                }
                if self
                    .main_path
                    .last()
                    .is_some_and(|n| !matches!(n.node, Node::Goto { .. }))
                {
                    self.main_path
                        .push(Stmt::new(None, Node::Goto { id: target }));
                }
            }
            _ => unreachable!(),
        }
    }

    fn first_dependency_label(&self, node: &ir::Node) -> Option<String> {
        match node {
            ir::Node::Atomic(id, _, _) => self
                .dependencies
                .get(id)
                .filter(|deps| !deps.is_empty())
                .map(|_| format!("L{id}")),
            ir::Node::Seq(children) | ir::Node::Par(children) => {
                for child in children {
                    if let Some(label) = self.first_dependency_label(child) {
                        return Some(label);
                    }
                }
                None
            }
            ir::Node::Dep(_) => None,
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
