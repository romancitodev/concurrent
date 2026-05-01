use std::collections::{HashMap, HashSet};
use std::fmt::{self, Write};
use std::marker::PhantomData;

use crate::{ValidationError, ValidationErrorKind};
use log::warn;

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

    pub fn to_par(self) -> Result<Graph<par::Node, Par, S>, crate::Error> {
        if has_dependencies(&self.0) {
            return Err(crate::Error::InvalidGraph(vec![
                crate::ValidationError::new(
                    crate::ValidationErrorKind::UnsupportedDependencies,
                    "Par cannot represent dependencies".to_string(),
                ),
            ]));
        }
        let ir_graph = ir::Graph::new(self.0);
        Ok(Graph::new(ir_graph.to_par().0))
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
        if let Err(errors) = validate_fk_labels(&g.0) {
            return Err(crate::Error::InvalidGraph(errors));
        }
        Ok(Graph::new(g.0))
    }

    pub fn to_ir(self) -> Graph<ir::Node, Ir, S> {
        let fk_graph = fk::Graph::new(self.0);
        let g = Graph::new(fk_graph.to_ir().0);
        println!("{g:?}");
        g
    }
}

fn has_dependencies(nodes: &[ir::Node]) -> bool {
    nodes.iter().any(has_dependencies_node)
}

fn has_dependencies_node(node: &ir::Node) -> bool {
    match node {
        ir::Node::Atomic(_, deps, _) => !deps.is_empty(),
        ir::Node::Par(children) | ir::Node::Seq(children) => {
            children.iter().any(has_dependencies_node)
        }
        ir::Node::Dep(_) => true,
    }
}

fn validate_fk_labels(stmts: &[fk::Stmt]) -> Result<(), Vec<ValidationError>> {
    let mut defined: HashMap<String, Vec<(usize, &'static str)>> = HashMap::new();
    let mut referenced: HashMap<String, Vec<(usize, &'static str)>> = HashMap::new();

    for (idx, stmt) in stmts.iter().enumerate() {
        if let Some(label) = &stmt.label {
            defined
                .entry(label.clone())
                .or_default()
                .push((idx, node_kind(&stmt.node)));
        }

        match &stmt.node {
            fk::Node::Goto { id } => {
                referenced
                    .entry(id.clone())
                    .or_default()
                    .push((idx, "goto"));
            }
            fk::Node::Fork { id } => {
                referenced
                    .entry(id.clone())
                    .or_default()
                    .push((idx, "fork"));
            }
            _ => {}
        }
    }

    let mut errors = Vec::new();
    for (label, sources) in &referenced {
        if !defined.contains_key(label) {
            let refs = sources
                .iter()
                .map(|(idx, kind)| format!("{kind}@#{idx}"))
                .collect::<Vec<_>>()
                .join(", ");
            let message = format!("Label '{label}' referenced by {refs} but not defined",);
            errors.push(ValidationError::new(
                ValidationErrorKind::MissingLabel,
                message.clone(),
            ));
            warn!("{message}");
        }
    }

    for (label, defs) in &defined {
        if label == "_end" {
            continue;
        }
        if !referenced.contains_key(label) {
            let defs = defs
                .iter()
                .map(|(idx, kind)| format!("{kind}@#{idx}"))
                .collect::<Vec<_>>()
                .join(", ");
            let message = format!("Label '{label}' defined at {defs} but never referenced",);
            errors.push(ValidationError::new(
                ValidationErrorKind::UnusedLabel,
                message.clone(),
            ));
            warn!("{message}");
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn node_kind(node: &fk::Node) -> &'static str {
    match node {
        fk::Node::Final => "final",
        fk::Node::Join { .. } => "join",
        fk::Node::Goto { .. } => "goto",
        fk::Node::Fork { .. } => "fork",
        fk::Node::Atomic { .. } => "atomic",
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
                fk::Node::Final => writeln!(f, "end")?,
                fk::Node::Atomic { id: name } => writeln!(f, "{name}")?,
                fk::Node::Fork { id: target } => writeln!(f, "fork {target}")?,
                fk::Node::Goto { id: target } => {
                    writeln!(f, "goto {target}")?;
                    in_branch = false;
                }
                fk::Node::Join { id: target } => {
                    writeln!(f, "join {target}")?;
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
