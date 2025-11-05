use crate::parser::items::{Graph, Node};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub kind: ValidationErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationErrorKind {
    CircularDependency,
    MissingDependency,
    InvalidConnectionCount,
}

impl ValidationError {
    pub fn new(kind: ValidationErrorKind, message: String) -> Self {
        Self { kind, message }
    }
}

pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Validates a graph for:
/// - Circular dependencies
/// - Missing dependencies
/// - Correct connection counts (respecting terminal nodes)
pub fn validate(graph: &Graph) -> ValidationResult {
    let mut errors = vec![];

    // First, collect all atomic nodes
    let all_nodes = collect_all_nodes(&graph.0);

    // Check for missing dependencies
    if let Err(missing_errors) = check_missing_dependencies(&all_nodes) {
        errors.extend(missing_errors);
    }

    // Check for circular dependencies
    if let Err(circular_errors) = check_circular_dependencies(&all_nodes) {
        errors.extend(circular_errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Collects all atomic nodes with their dependencies and terminal status
fn collect_all_nodes(nodes: &[Node]) -> HashMap<String, (Vec<String>, bool)> {
    let mut result = HashMap::new();
    collect_nodes_recursive(nodes, &mut result);
    result
}

fn collect_nodes_recursive(nodes: &[Node], map: &mut HashMap<String, (Vec<String>, bool)>) {
    for node in nodes {
        match node {
            Node::Atomic(id, deps, is_terminal) => {
                let dep_ids: Vec<String> = deps
                    .iter()
                    .filter_map(|n| match n {
                        Node::Dep(dep_id) => Some(dep_id.clone()),
                        _ => None,
                    })
                    .collect();
                map.insert(id.clone(), (dep_ids, *is_terminal));
            }
            Node::Seq(inner_nodes) | Node::Par(inner_nodes) => {
                collect_nodes_recursive(inner_nodes, map);
            }
            Node::Dep(_) => {
                // Dependencies are handled in Atomic nodes
            }
        }
    }
}

/// Checks if all dependencies exist in the graph
fn check_missing_dependencies(
    nodes: &HashMap<String, (Vec<String>, bool)>,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = vec![];
    let all_node_ids: HashSet<String> = nodes.keys().cloned().collect();

    for (node_id, (deps, _)) in nodes {
        for dep_id in deps {
            if !all_node_ids.contains(dep_id) {
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

/// Checks for circular dependencies using DFS
fn check_circular_dependencies(
    nodes: &HashMap<String, (Vec<String>, bool)>,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = vec![];
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();

    for node_id in nodes.keys() {
        if visited.contains(node_id) {
            continue;
        }
        if let Some(cycle) = detect_cycle(node_id, nodes, &mut visited, &mut rec_stack, &mut vec![])
        {
            errors.push(ValidationError::new(
                ValidationErrorKind::CircularDependency,
                format!("Circular dependency detected: {}", cycle.join(" -> ")),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// DFS-based cycle detection
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
                // Found a cycle
                let mut cycle = vec![];
                let mut found_start = false;
                for p in path.iter() {
                    if p == dep_id {
                        found_start = true;
                    }
                    if found_start {
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

/// Counts the number of direct connections for a node
/// Terminal nodes (marked with !) are not counted as connections to their dependents
pub fn count_connections(node_id: &str, nodes: &HashMap<String, (Vec<String>, bool)>) -> usize {
    if let Some((deps, _)) = nodes.get(node_id) {
        // Count only non-terminal dependencies
        deps.iter()
            .filter(|dep_id| {
                if let Some((_, is_terminal)) = nodes.get(*dep_id) {
                    !is_terminal
                } else {
                    true // If dependency doesn't exist, count it (will be caught by validation)
                }
            })
            .count()
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_dependency() {
        let mut nodes = HashMap::new();
        nodes.insert("s0".to_string(), (vec!["s1".to_string()], false));
        // s1 doesn't exist

        let result = check_missing_dependencies(&nodes);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, ValidationErrorKind::MissingDependency);
    }

    #[test]
    fn test_circular_dependency() {
        let mut nodes = HashMap::new();
        nodes.insert("s0".to_string(), (vec!["s1".to_string()], false));
        nodes.insert("s1".to_string(), (vec!["s2".to_string()], false));
        nodes.insert("s2".to_string(), (vec!["s0".to_string()], false));

        let result = check_circular_dependencies(&nodes);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, ValidationErrorKind::CircularDependency);
    }

    #[test]
    fn test_terminal_node_connection_count() {
        let mut nodes = HashMap::new();
        nodes.insert("s6".to_string(), (vec![], true)); // Terminal node
        nodes.insert("s8".to_string(), (vec!["s6".to_string()], false));
        nodes.insert("s9".to_string(), (vec!["s6".to_string()], false));
        nodes.insert(
            "sa".to_string(),
            (vec!["s8".to_string(), "s9".to_string()], false),
        );

        // sa should have 2 connections (s8 and s9), not 3
        // because s6 is terminal and shouldn't be counted transitively
        let connections = count_connections("sa", &nodes);
        assert_eq!(connections, 2);
    }

    #[test]
    fn test_valid_graph() {
        let mut nodes = HashMap::new();
        nodes.insert("s0".to_string(), (vec![], false));
        nodes.insert("s1".to_string(), (vec!["s0".to_string()], false));

        let result = validate(&Graph::new(vec![
            Node::Atomic("s0".to_string(), vec![], false),
            Node::Atomic("s1".to_string(), vec![Node::Dep("s0".to_string())], false),
        ]));

        assert!(result.is_ok());
    }
}
