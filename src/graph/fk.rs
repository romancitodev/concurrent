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
        conv.convert_nodes(&ir.0);
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
        self.main_path.push(Stmt::new(None, Node::Goto { id: "_end".to_string() }));
        self.main_path.push(Stmt::new(Some("_end".to_string()), Node::Final));
        for branch in branches {
            self.expand_branch(branch.stmts);
        }
        Graph::new(self.main_path)
    }

    fn convert_nodes(&mut self, nodes: &[ir::Node]) {
        for node in nodes {
            self.convert_node(node, None);
        }
    }

    fn update_counter(&mut self) -> usize {
        let current = self.label_counter;
        self.label_counter += 1;
        current
    }

    fn convert_node(&mut self, node: &ir::Node, label: Option<String>) {
        match node {
            ir::Node::Atomic(name, deps, _) => {
                // because we are resolving the deps after the atomic node itself, the behaviour is gonna be bottom-to-top.
                // this means that if we have the node B and C that precedes A, when we analyze B as a dep, we need the parent node.
                // but that means we need to keep also a field like `from` and `to`, and we need to keep track of the current node we are analyzing, and pass it to the recursive calls.
                let counter = format!("c{}", self.update_counter());
                self.resolve_dependencies(name, counter, deps.as_ref());

                self.main_path
                    .push(Stmt::new(label.clone(), Node::Atomic { id: name.clone() }));
                // if *is_terminal {
                //     self.main_path
                //         .push(Stmt::new(label, Node::Final));
                // }
            }
            ir::Node::Seq(children) => {
                self.convert_nodes(children);
            }
            ir::Node::Par(branches) => {
                self.convert_parallel(branches);
            }
            // We now that the only way to have a `Dep` node is as a dependency of an `Atomic` node, and we are already handling that case by recursively converting the dependencies before the atomic node itself.
            ir::Node::Dep(id) => { }
        }
    }

    fn resolve_dependencies(&mut self, parent: &String, counter: String, deps: &[ir::Node]) {
      if deps.is_empty() { return; }
      let label = format!("L{parent}");
      self.main_path.push(Stmt::new(Some(label.clone()), Node::Join { id: counter }));
      for dep in deps {
        assert!(matches!(dep, ir::Node::Dep(_)), "Only Dep nodes are allowed as dependencies");
        self.dependencies.entry(parent.clone()).or_default().push(dep.id());
      }
    }

    /// branches is the list of branches that we need to convert in parallel, and `to` is the label of the node that we need to join to after the branches are done.
    fn convert_parallel(&mut self, branches: &[ir::Node]) {
      if branches.is_empty() { return; }

      let join = format!("L{}", self.label_counter); // `self.label_counter` for example.
      let forks = branches.iter().skip(1).map(|n| format!("L{}", n.id())).collect::<Vec<_>>();

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
        self.main_path.push(Stmt::new(None, Node::Fork { id: fork }));
      }

      // We are going to take the first branch as the main. (the most-left branch will be the "main" path always).
      let main_branch = &branches[0];
      self.convert_node(main_branch, None);

      for branch in &branches[1..] {
        let label = self.new_label();
        self.branches.push(Branch {
          label, // L{unknown}
          stmts: branch.clone(), // the entire node.
          target: join.clone(), // join LF.
        });
      }

    }

    fn expand_branch(&mut self, branch: ir::Node) {
        match branch {
          ir::Node::Atomic(label, _, _) => {
            self.dependencies
              .iter()
              .filter(|(_, v)| v.contains(&label))
              .for_each(|(k, _)| {
                self.main_path.push(Stmt::new(None, Node::Goto { id: k.clone() }));
              });
          },
          ir::Node::Par(branch) | ir::Node::Seq(branch) => {
            for node in branch {
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
                let label = format!("L{}", Self::first_node_name(&node));
                self.convert_node(&node, Some(label));
                self.dependencies
                  .iter()
                  .filter(|(_, v)| v.contains(&node.id()))
                  .for_each(|(k, _)| {
                    self.main_path.push(Stmt::new(None, Node::Fork { id: k.clone() }));
                  });
            }
          },
          _ => unreachable!()
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
