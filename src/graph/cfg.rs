use std::collections::{HashMap, HashSet};

use super::fk;
use super::ir;

#[derive(Debug, Clone)]
enum Region {
    Atomic { name: String },
    Sequence { regions: Vec<Region> },
    Parallel { branches: Vec<Region> },
}

impl Region {
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

    fn is_empty(&self) -> bool {
        matches!(self, Region::Sequence { regions } if regions.is_empty())
    }
}

#[derive(Debug)]
pub struct ControlFlowGraph {
    nodes: HashMap<usize, fk::Node>,
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

    pub fn from_graph(graph: &fk::Graph) -> Self {
        let mut cfg = ControlFlowGraph::new();

        for (idx, stmt) in graph.0.iter().enumerate() {
            if let Some(label) = &stmt.label {
                cfg.labels.insert(label.clone(), idx);
            }
            cfg.nodes.insert(idx, stmt.node.clone());
        }

        for (idx, stmt) in graph.0.iter().enumerate() {
            match &stmt.node {
                fk::Node::Goto(target_label) => {
                    if target_label != "end"
                        && let Some(&target_idx) = cfg.labels.get(target_label)
                    {
                        cfg.edges.push((idx, target_idx));
                    }
                }
                fk::Node::Fork(target_label) => {
                    if let Some(&target_idx) = cfg.labels.get(target_label) {
                        cfg.edges.push((idx, target_idx));
                    }
                    if idx + 1 < graph.0.len() {
                        cfg.edges.push((idx, idx + 1));
                    }
                }
                fk::Node::Atomic(_) | fk::Node::Join(_) => {
                    if idx + 1 < graph.0.len() {
                        cfg.edges.push((idx, idx + 1));
                    }
                }
            }
        }

        cfg
    }

    pub fn to_ir(&self) -> ir::Graph {
        let region = self.build_region_tree(0, &mut HashSet::new(), 0);
        let ir_nodes = Self::region_to_ir(&region);
        ir::Graph::new(vec![ir_nodes])
    }

    fn build_region_tree(
        &self,
        start_idx: usize,
        visited: &mut HashSet<usize>,
        depth: usize,
    ) -> Region {
        const MAX_DEPTH: usize = 1000;

        if depth > MAX_DEPTH {
            return Region::Sequence { regions: vec![] };
        }

        if visited.contains(&start_idx) {
            return Region::Sequence { regions: vec![] };
        }

        let Some(node) = self.nodes.get(&start_idx) else {
            return Region::Sequence { regions: vec![] };
        };

        visited.insert(start_idx);

        match node {
            fk::Node::Fork(target_label) => {
                self.build_fork_region(start_idx, target_label, visited, depth)
            }
            fk::Node::Atomic(name) => {
                let atomic_region = Region::Atomic { name: name.clone() };

                if let Some(next_idx) = self.find_successor(start_idx)
                    && !visited.contains(&next_idx)
                {
                    let next_region = self.build_region_tree(next_idx, visited, depth + 1);
                    return Region::sequence(vec![atomic_region, next_region]);
                }
                atomic_region
            }
            fk::Node::Join(_) | fk::Node::Goto(_) => {
                if let Some(next_idx) = self.find_successor(start_idx)
                    && !visited.contains(&next_idx)
                {
                    return self.build_region_tree(next_idx, visited, depth + 1);
                }
                Region::Sequence { regions: vec![] }
            }
        }
    }

    fn build_fork_region(
        &self,
        fork_idx: usize,
        target_label: &str,
        visited: &mut HashSet<usize>,
        depth: usize,
    ) -> Region {
        let continuation_idx = fork_idx + 1;
        let target_idx = self.labels.get(target_label).copied();

        let mut branches = Vec::new();

        let cont_region = self.build_branch_to_end(continuation_idx, visited, depth);
        if !cont_region.is_empty() {
            branches.push(cont_region);
        }

        if let Some(target) = target_idx {
            let target_region = self.build_branch_to_end(target, visited, depth);
            if !target_region.is_empty() {
                branches.push(target_region);
            }
        }

        Region::Parallel { branches }
    }

    fn build_branch_to_end(
        &self,
        start_idx: usize,
        visited: &mut HashSet<usize>,
        depth: usize,
    ) -> Region {
        let mut local_visited = visited.clone();
        let region = self.build_region_until_terminal(start_idx, &mut local_visited, depth);
        visited.extend(local_visited);
        region
    }

    fn build_region_until_terminal(
        &self,
        start_idx: usize,
        visited: &mut HashSet<usize>,
        depth: usize,
    ) -> Region {
        const MAX_DEPTH: usize = 1000;
        const MAX_ITERATIONS: usize = 10000;

        if depth > MAX_DEPTH {
            return Region::Sequence { regions: vec![] };
        }

        let mut regions = Vec::new();
        let mut current = start_idx;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                break;
            }

            if visited.contains(&current) {
                break;
            }

            let Some(node) = self.nodes.get(&current) else {
                break;
            };

            visited.insert(current);

            match node {
                fk::Node::Fork(target_label) => {
                    regions.push(self.build_fork_region(current, target_label, visited, depth + 1));
                    break;
                }
                fk::Node::Atomic(name) => {
                    regions.push(Region::Atomic { name: name.clone() });
                }
                fk::Node::Join(_) | fk::Node::Goto(_) => {}
            }

            let Some(next) = self.find_successor(current) else {
                break;
            };
            current = next;
        }

        Region::sequence(regions)
    }

    fn region_to_ir(region: &Region) -> ir::Node {
        match region {
            Region::Atomic { name } => ir::Node::Atomic(name.clone(), vec![], false),
            Region::Sequence { regions } => {
                let ir_nodes: Vec<_> = regions.iter().map(Self::region_to_ir).collect();
                match ir_nodes.len() {
                    0 => ir::Node::Seq(vec![]),
                    1 => ir_nodes.into_iter().next().unwrap(),
                    _ => ir::Node::Seq(ir_nodes),
                }
            }
            Region::Parallel { branches } => {
                let ir_branches: Vec<_> = branches.iter().map(Self::region_to_ir).collect();
                ir::Node::Par(ir_branches)
            }
        }
    }

    fn find_successor(&self, idx: usize) -> Option<usize> {
        self.edges
            .iter()
            .find(|(from, _)| *from == idx)
            .map(|(_, to)| *to)
    }
}
