use crate::parser::items::{Graph, Node};
use petgraph::Directed;
use petgraph::graph::{Graph as PetGraph, NodeIndex};
use std::collections::HashMap;

pub(crate) type Flow = PetGraph<String, &'static str, Directed>;

impl Graph {
    pub fn to_petgraph(&self) -> Flow {
        let mut graph = Flow::new();
        let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

        // First pass: collect all atomic nodes
        for node in &self.0 {
            collect_nodes(node, &mut graph, &mut node_map);
        }

        // Second pass: build connections based on structure
        // The graph root is a sequence of nodes (separated by commas)
        let mut current_parents: Vec<NodeIndex> = Vec::with_capacity(4);
        for node in &self.0 {
            current_parents = build_connections(node, &mut graph, &node_map, current_parents);
        }

        graph
    }
}

/// Collect all the atomic nodes
fn collect_nodes(node: &Node, g: &mut Flow, node_map: &mut HashMap<String, NodeIndex>) {
    match node {
        Node::Par(nodes) | Node::Seq(nodes) => {
            for n in nodes {
                collect_nodes(n, g, node_map);
            }
        }
        Node::Atomic(id, deps) => {
            node_map
                .entry(id.clone())
                .or_insert_with(|| g.add_node(id.clone()));
            for dep in deps {
                collect_nodes(dep, g, node_map);
            }
        }
        Node::Dep(name) => {
            node_map
                .entry(name.clone())
                .or_insert_with(|| g.add_node(name.clone()));
        }
    }
}

/// Returns the list of "last nodes" that completed in this branch
fn build_connections(
    node: &Node,
    g: &mut Flow,
    node_map: &HashMap<String, NodeIndex>,
    parents: Vec<NodeIndex>,
) -> Vec<NodeIndex> {
    match node {
        Node::Par(nodes) => {
            // Parallel: all nodes start from the same parent(s)
            // Collect all their ending nodes
            let mut all_endings = Vec::with_capacity(nodes.len());

            for n in nodes {
                let endings = build_connections(n, g, node_map, parents.clone());
                all_endings.extend(endings);
            }

            all_endings
        }
        Node::Seq(nodes) => {
            // Sequence: each node starts after the previous one finishes
            let mut current_parents = parents;

            for n in nodes {
                current_parents = build_connections(n, g, node_map, current_parents);
            }

            current_parents
        }
        Node::Atomic(id, deps) => {
            let current_index = node_map[id];

            // Connect parent nodes to this node
            for &parent_index in &parents {
                g.add_edge(parent_index, current_index, "");
            }

            // Process dependencies
            for dep in deps {
                if let Node::Dep(dep_name) = dep {
                    let dep_index = node_map[dep_name];
                    g.add_edge(dep_index, current_index, "");
                }
            }

            // Return this node as the ending node
            vec![current_index]
        }
        Node::Dep(name) => {
            // Dependencies are handled in Atomic nodes
            // This shouldn't be called directly in the main tree
            vec![node_map[name]]
        }
    }
}
