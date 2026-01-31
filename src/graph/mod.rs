use std::fmt::{self, Write};
use std::marker::PhantomData;

mod cfg;
pub mod fk;
pub mod ir;
pub mod par;

pub use ir::Node as IrNode;

pub struct Ir;
pub struct Par;
pub struct ForkJoin;

pub struct Valid;
pub struct Unvalidated;

pub struct Graph<N, K, S = Unvalidated>(pub Vec<N>, PhantomData<K>, PhantomData<S>);

impl<N, K, S> Graph<N, K, S> {
    pub fn new(nodes: Vec<N>) -> Self {
        Self(nodes, PhantomData, PhantomData)
    }

    pub fn nodes(&self) -> &[N] {
        &self.0
    }
}

impl<S> Graph<ir::Node, Ir, S> {
    pub fn parse(input: &str) -> Result<Self, crate::Error> {
        let g = ir::parse(input).map_err(|e| crate::Error::ParseError(format!("IR: {e}")))?;
        Ok(Graph::new(g.0))
    }

    pub fn to_fk(self) -> Graph<fk::Stmt, ForkJoin, S> {
        let ir_graph = ir::Graph::new(self.0);
        Graph::new(ir_graph.to_fk().0)
    }

    pub fn to_par(self) -> Graph<par::Node, Par, S> {
        let ir_graph = ir::Graph::new(self.0);
        Graph::new(ir_graph.to_par().0)
    }
}

impl<S> Graph<par::Node, Par, S> {
    pub fn parse(input: &str) -> Result<Self, crate::Error> {
        let g = par::parse(input).map_err(|e| crate::Error::ParseError(format!("Par: {e}")))?;
        Ok(Graph::new(g.0))
    }

    pub fn to_ir(self) -> Graph<ir::Node, Ir, S> {
        let par_graph = par::Graph::new(self.0);
        Graph::new(par_graph.to_ir().0)
    }
}

impl<S> Graph<fk::Stmt, ForkJoin, S> {
    pub fn parse(input: &str) -> Result<Self, crate::Error> {
        let g = fk::parse(input).map_err(|e| crate::Error::ParseError(format!("ForkJoin: {e}")))?;
        Ok(Graph::new(g.0))
    }

    pub fn to_ir(self) -> Graph<ir::Node, Ir, S> {
        let fk_graph = fk::Graph::new(self.0);
        Graph::new(fk_graph.to_ir().0)
    }
}

fn format_node(node: &ir::Node) -> String {
    match node {
        ir::Node::Par(nodes) => {
            let inner = nodes.iter().map(format_node).collect::<Vec<_>>().join(",");
            format!("{{{inner}}}")
        }
        ir::Node::Seq(nodes) => {
            let inner = nodes.iter().map(format_node).collect::<Vec<_>>().join(",");
            format!("[{inner}]")
        }
        ir::Node::Atomic(name, deps, terminal) => {
            let mut result = name.clone();
            if !deps.is_empty() {
                let dep_names = deps
                    .iter()
                    .filter_map(|d| match d {
                        ir::Node::Dep(n) => Some(n.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                write!(&mut result, "#{{{dep_names}}}").unwrap();
            }
            if *terminal {
                result.push('!');
            }
            result
        }
        ir::Node::Dep(name) => name.clone(),
    }
}

impl<S> fmt::Display for Graph<ir::Node, Ir, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.0.iter().map(format_node).collect::<Vec<_>>().join(",");
        write!(f, "${inner}$")
    }
}

impl<S> fmt::Display for Graph<par::Node, Par, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "begin")?;
        for node in &self.0 {
            format_par_node(f, node, 1)?;
        }
        write!(f, "end")
    }
}

fn format_par_node(f: &mut fmt::Formatter<'_>, node: &par::Node, indent: usize) -> fmt::Result {
    let pad = "  ".repeat(indent);
    match node {
        par::Node::Atomic(name) => writeln!(f, "{pad}{name}"),
        par::Node::Par(children) => {
            writeln!(f, "{pad}parbegin")?;
            for child in children {
                format_par_node(f, child, indent + 1)?;
            }
            writeln!(f, "{pad}parend")
        }
        par::Node::Seq(children) => {
            writeln!(f, "{pad}begin")?;
            for child in children {
                format_par_node(f, child, indent + 1)?;
            }
            writeln!(f, "{pad}end")
        }
    }
}

impl<S> fmt::Display for Graph<fk::Stmt, ForkJoin, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "begin")?;
        let mut in_branch = false;
        for stmt in &self.0 {
            let indent = if in_branch { "        " } else { "    " };
            if let Some(label) = &stmt.label {
                write!(f, "    {label}: ")?;
                in_branch = true;
            } else {
                write!(f, "{indent}")?;
            }
            match &stmt.node {
                fk::Node::Atomic(name) => writeln!(f, "{name}")?,
                fk::Node::Fork(target) => writeln!(f, "fork {target}")?,
                fk::Node::Goto(target) => {
                    writeln!(f, "goto {target}")?;
                    in_branch = false;
                }
                fk::Node::Join(Some(target)) => {
                    writeln!(f, "join {target}")?;
                    in_branch = false;
                }
                fk::Node::Join(None) => {
                    writeln!(f, "join")?;
                    in_branch = false;
                }
            }
        }
        write!(f, "end")
    }
}

impl<N: std::fmt::Debug, K, S> std::fmt::Debug for Graph<N, K, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Graph").field(&self.0).finish()
    }
}
