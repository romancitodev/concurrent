use std::collections::{HashMap, HashSet};

use crate::parser::fk::items::{Graph, Node};
use crate::parser::ir::items as grammar;

/// Represents a region in the control flow graph
#[derive(Debug, Clone)]
enum Region {
    /// Sequential region containing a single atomic operation
    Atomic { /*idx: usize,*/ name: String },
    /// Sequential region containing multiple sub-regions
    Sequence { regions: Vec<Region> },
    /// Parallel region with multiple branches that converge at a join
    Parallel {
        branches: Vec<Region>,
        /* join_idx: Option<usize>, */
    },
}

impl Region {
    /// Creates a sequence region, automatically unwrapping single-element sequences
    fn sequence(regions: Vec<Region>) -> Self {
        let regions: Vec<_> = regions
            .into_iter()
            .filter(|r| !matches!(r, Region::Sequence { regions } if regions.is_empty()))
            .collect();

        match regions.len() {
            0 => Region::Sequence { regions: vec![] },
            1 => regions.into_iter().next().unwrap(),
            _ => Region::Sequence { regions },
        }
    }

    /// Checks if this region is empty
    fn is_empty(&self) -> bool {
        matches!(self, Region::Sequence { regions } if regions.is_empty())
    }
}

#[derive(Debug)]
pub struct ControlFlowGraph {
    nodes: HashMap<usize, Node>,
    edges: Vec<(usize, usize)>,
    labels: HashMap<String, usize>,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            labels: HashMap::new(),
        }
    }

    pub fn from_graph(graph: &Graph) -> Self {
        let mut cfg = ControlFlowGraph::new();

        // First pass: collect labels
        for (idx, stmt) in graph.0.iter().enumerate() {
            if let Some(label) = &stmt.label {
                cfg.labels.insert(label.clone(), idx);
            }
            cfg.nodes.insert(idx, stmt.node.clone());
        }

        // Second pass: build edges
        for (idx, stmt) in graph.0.iter().enumerate() {
            match &stmt.node {
                Node::Goto(target_label) => {
                    // "goto end" is terminal (no edge)
                    if target_label != "end"
                        && let Some(&target_idx) = cfg.labels.get(target_label)
                    {
                        cfg.edges.push((idx, target_idx));
                    }
                }
                Node::Fork(target_label) => {
                    // Fork creates two edges: to target and to next statement
                    if let Some(&target_idx) = cfg.labels.get(target_label) {
                        cfg.edges.push((idx, target_idx));
                    }
                    if idx + 1 < graph.0.len() {
                        cfg.edges.push((idx, idx + 1));
                    }
                }
                Node::Atomic(_) | Node::Join(_) => {
                    // Sequential flow to next statement
                    if idx + 1 < graph.0.len() {
                        cfg.edges.push((idx, idx + 1));
                    }
                }
            }
        }

        cfg
    }

    /// Converts the fork-join CFG to the grammar IR
    pub fn to_ir(&self) -> grammar::Graph {
        let region = self.build_region_tree(0, &mut HashSet::new());
        let ir_nodes = Self::region_to_ir(&region);
        grammar::Graph::new(vec![ir_nodes])
    }

    /// Builds a hierarchical region tree from the CFG
    fn build_region_tree(&self, start_idx: usize, visited: &mut HashSet<usize>) -> Region {
        if visited.contains(&start_idx) {
            return Region::Sequence { regions: vec![] };
        }

        let Some(node) = self.nodes.get(&start_idx) else {
            return Region::Sequence { regions: vec![] };
        };

        visited.insert(start_idx);

        match node {
            Node::Fork(target_label) => self.build_fork_region(start_idx, target_label, visited),
            Node::Atomic(name) => {
                let atomic_region = Region::Atomic { name: name.clone() };

                if let Some(next_idx) = self.find_successor(start_idx)
                    && !visited.contains(&next_idx)
                {
                    let next_region = self.build_region_tree(next_idx, visited);
                    return Region::sequence(vec![atomic_region, next_region]);
                }
                atomic_region
            }
            Node::Join(_) | Node::Goto(_) => {
                // Continue through structural nodes
                if let Some(next_idx) = self.find_successor(start_idx)
                    && !visited.contains(&next_idx)
                {
                    return self.build_region_tree(next_idx, visited);
                }
                Region::Sequence { regions: vec![] }
            }
        }
    }

    /// Builds a parallel region from a fork instruction
    fn build_fork_region(
        &self,
        fork_idx: usize,
        target_label: &str,
        visited: &mut HashSet<usize>,
    ) -> Region {
        let continuation_idx = fork_idx + 1;
        let target_idx = self.labels.get(target_label).copied();

        let mut branches = Vec::new();

        // Build both branches to their natural termination
        // Build continuation branch
        let cont_region = self.build_branch_to_end(continuation_idx, visited);
        if !cont_region.is_empty() {
            branches.push(cont_region);
        }

        // Build target branch
        if let Some(target) = target_idx {
            let target_region = self.build_branch_to_end(target, visited);
            if !target_region.is_empty() {
                branches.push(target_region);
            }
        }

        Region::Parallel { branches }
    }

    /// Builds a branch region to its natural end
    fn build_branch_to_end(&self, start_idx: usize, visited: &mut HashSet<usize>) -> Region {
        let mut local_visited = HashSet::new();
        let region = self.build_region_until_terminal(start_idx, &mut local_visited);

        // Merge local visited into global
        visited.extend(local_visited);
        region
    }

    /// Builds a region from start until a terminal node
    fn build_region_until_terminal(
        &self,
        start_idx: usize,
        visited: &mut HashSet<usize>,
    ) -> Region {
        let mut regions = Vec::new();
        let mut current = start_idx;

        loop {
            if visited.contains(&current) {
                break;
            }

            let Some(node) = self.nodes.get(&current) else {
                break;
            };

            visited.insert(current);

            match node {
                Node::Fork(target_label) => {
                    regions.push(self.build_fork_region(current, target_label, visited));
                    break; // Fork handles its own continuation
                }
                Node::Atomic(name) => {
                    regions.push(Region::Atomic { name: name.clone() });
                }
                Node::Join(_) | Node::Goto(_) => {
                    // Continue through structural nodes
                    // "goto end" is handled by find_successor returning None
                }
            }

            let Some(next) = self.find_successor(current) else {
                break;
            };
            current = next;
        }

        Region::sequence(regions)
    }

    /// Converts a region tree to IR nodes
    fn region_to_ir(region: &Region) -> grammar::Node {
        match region {
            Region::Atomic { name, .. } => grammar::Node::Atomic(name.clone(), vec![], false),
            Region::Sequence { regions } => {
                let ir_nodes: Vec<_> = regions.iter().map(Self::region_to_ir).collect();
                match ir_nodes.len() {
                    0 => grammar::Node::Seq(vec![]),
                    1 => ir_nodes.into_iter().next().unwrap(),
                    _ => grammar::Node::Seq(ir_nodes),
                }
            }
            Region::Parallel { branches, .. } => {
                let ir_branches: Vec<_> = branches.iter().map(Self::region_to_ir).collect();
                grammar::Node::Par(ir_branches)
            }
        }
    }

    /// Finds the successor of a node in the CFG
    fn find_successor(&self, idx: usize) -> Option<usize> {
        self.edges
            .iter()
            .find(|(from, _)| *from == idx)
            .map(|(_, to)| *to)
    }
}
