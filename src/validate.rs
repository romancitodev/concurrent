use std::collections::{HashMap, HashSet};

use crate::error::{ValidationError, ValidationErrorKind};
use crate::graph::{Graph, Ir, Unvalidated, Valid, ir};

pub type ValidationResult<T = ()> = Result<T, Vec<ValidationError>>;

impl Graph<ir::Node, Ir, Unvalidated> {
    pub fn validate(self) -> ValidationResult<Graph<ir::Node, Ir, Valid>> {
        let mut errors = vec![];
        let nodes = collect_all_nodes(&self.0);

        if let Err(missing) = check_missing_dependencies(&nodes) {
            errors.extend(missing);
        }

        if let Err(circular) = check_circular_dependencies(&nodes) {
            errors.extend(circular);
        }

        if errors.is_empty() {
            Ok(Graph::new(self.0))
        } else {
            Err(errors)
        }
    }
}

impl Graph<ir::Node, Ir, Valid> {
    pub fn to_petgraph(&self) -> petgraph::Graph<String, &'static str> {
        let mut graph = petgraph::Graph::new();
        let mut node_indices = HashMap::new();

        add_nodes_to_petgraph(&self.0, &mut graph, &mut node_indices, &[]);
        add_edges_to_petgraph(&self.0, &mut graph, &node_indices);

        graph
    }
}

fn add_nodes_to_petgraph(
    nodes: &[ir::Node],
    graph: &mut petgraph::Graph<String, &'static str>,
    node_indices: &mut HashMap<String, petgraph::graph::NodeIndex>,
    parents: &[petgraph::graph::NodeIndex],
) {
    let mut prev: Vec<petgraph::graph::NodeIndex> = parents.to_vec();

    for node in nodes {
        match node {
            ir::Node::Atomic(id, _deps, is_terminal) => {
                let idx = graph.add_node(id.clone());
                node_indices.insert(id.clone(), idx);

                for p in &prev {
                    graph.add_edge(*p, idx, "");
                }

                prev = if *is_terminal { vec![] } else { vec![idx] };
            }
            ir::Node::Seq(inner_nodes) => {
                add_nodes_to_petgraph(inner_nodes, graph, node_indices, &prev);
                prev = get_last_indices(inner_nodes, node_indices);
            }
            ir::Node::Par(branches) => {
                let mut all_last = vec![];
                for branch in branches {
                    add_nodes_to_petgraph(std::slice::from_ref(branch), graph, node_indices, &prev);
                    all_last.extend(get_last_index(branch, node_indices));
                }
                prev = all_last;
            }
            ir::Node::Dep(_) => {}
        }
    }
}

fn get_last_indices(
    nodes: &[ir::Node],
    node_indices: &HashMap<String, petgraph::graph::NodeIndex>,
) -> Vec<petgraph::graph::NodeIndex> {
    nodes
        .last()
        .map(|n| get_last_index(n, node_indices))
        .unwrap_or_default()
}

fn get_last_index(
    node: &ir::Node,
    node_indices: &HashMap<String, petgraph::graph::NodeIndex>,
) -> Vec<petgraph::graph::NodeIndex> {
    match node {
        ir::Node::Atomic(id, _, is_terminal) => {
            if *is_terminal {
                vec![]
            } else {
                node_indices.get(id).copied().into_iter().collect()
            }
        }
        ir::Node::Seq(nodes) => get_last_indices(nodes, node_indices),
        ir::Node::Par(branches) => branches
            .iter()
            .flat_map(|b| get_last_index(b, node_indices))
            .collect(),
        ir::Node::Dep(_) => vec![],
    }
}

fn add_edges_to_petgraph(
    nodes: &[ir::Node],
    graph: &mut petgraph::Graph<String, &'static str>,
    node_indices: &HashMap<String, petgraph::graph::NodeIndex>,
) {
    for node in nodes {
        match node {
            ir::Node::Atomic(id, deps, _) => {
                if let Some(target_idx) = node_indices.get(id) {
                    for dep in deps {
                        if let ir::Node::Dep(dep_id) = dep
                            && let Some(source_idx) = node_indices.get(dep_id)
                        {
                            graph.add_edge(*source_idx, *target_idx, "dep");
                        }
                    }
                }
            }
            ir::Node::Seq(inner) | ir::Node::Par(inner) => {
                add_edges_to_petgraph(inner, graph, node_indices);
            }
            ir::Node::Dep(_) => {}
        }
    }
}

fn collect_all_nodes(nodes: &[ir::Node]) -> HashMap<String, (Vec<String>, bool)> {
    let mut result = HashMap::new();
    collect_recursive(nodes, &mut result);
    result
}

fn collect_recursive(nodes: &[ir::Node], map: &mut HashMap<String, (Vec<String>, bool)>) {
    for node in nodes {
        match node {
            ir::Node::Atomic(id, deps, is_terminal) => {
                let dep_ids = deps
                    .iter()
                    .filter_map(|n| match n {
                        ir::Node::Dep(dep_id) => Some(dep_id.clone()),
                        _ => None,
                    })
                    .collect();
                map.insert(id.clone(), (dep_ids, *is_terminal));
            }
            ir::Node::Seq(inner) | ir::Node::Par(inner) => collect_recursive(inner, map),
            ir::Node::Dep(_) => {}
        }
    }
}

fn check_missing_dependencies(
    nodes: &HashMap<String, (Vec<String>, bool)>,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = vec![];
    let all_ids: HashSet<_> = nodes.keys().cloned().collect();

    for (node_id, (deps, _)) in nodes {
        for dep_id in deps {
            if !all_ids.contains(dep_id) {
                errors.push(ValidationError::new(
                    ValidationErrorKind::MissingDependency,
                    format!("Node '{node_id}' depends on '{dep_id}' which doesn't exist"),
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_circular_dependencies(
    nodes: &HashMap<String, (Vec<String>, bool)>,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = vec![];
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();

    for node_id in nodes.keys() {
        if !visited.contains(node_id)
            && let Some(cycle) =
                detect_cycle(node_id, nodes, &mut visited, &mut rec_stack, &mut vec![])
        {
            errors.push(ValidationError::new(
                ValidationErrorKind::CircularDependency,
                format!("Circular dependency: {}", cycle.join(" -> ")),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn detect_cycle(
    node_id: &str,
    nodes: &HashMap<String, (Vec<String>, bool)>,
    visited: &mut HashSet<String>,
    rec_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Option<Vec<String>> {
    visited.insert(node_id.to_string());
    rec_stack.insert(node_id.to_string());
    path.push(node_id.to_string());

    if let Some((deps, _)) = nodes.get(node_id) {
        for dep_id in deps {
            if !visited.contains(dep_id) {
                if let Some(cycle) = detect_cycle(dep_id, nodes, visited, rec_stack, path) {
                    return Some(cycle);
                }
            } else if rec_stack.contains(dep_id) {
                let mut cycle = vec![];
                let mut found = false;
                for p in path.iter() {
                    if p == dep_id {
                        found = true;
                    }
                    if found {
                        cycle.push(p.clone());
                    }
                }
                cycle.push(dep_id.clone());
                return Some(cycle);
            }
        }
    }

    rec_stack.remove(node_id);
    path.pop();
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_dependency() {
        let mut nodes = HashMap::new();
        nodes.insert("s0".to_string(), (vec!["s1".to_string()], false));

        let result = check_missing_dependencies(&nodes);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err()[0].kind,
            ValidationErrorKind::MissingDependency
        );
    }

    #[test]
    fn test_circular_dependency() {
        let mut nodes = HashMap::new();
        nodes.insert("s0".to_string(), (vec!["s1".to_string()], false));
        nodes.insert("s1".to_string(), (vec!["s2".to_string()], false));
        nodes.insert("s2".to_string(), (vec!["s0".to_string()], false));

        let result = check_circular_dependencies(&nodes);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err()[0].kind,
            ValidationErrorKind::CircularDependency
        );
    }

    #[test]
    fn test_valid_graph() {
        let result = Graph::<ir::Node, Ir, Unvalidated>::new(vec![
            ir::Node::Atomic("s0".to_string(), vec![], false),
            ir::Node::Atomic(
                "s1".to_string(),
                vec![ir::Node::Dep("s0".to_string())],
                false,
            ),
        ])
        .validate();

        assert!(result.is_ok());
    }
}
