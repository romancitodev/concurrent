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
        // Flatten nested sequences and remove empty ones
        let regions: Vec<_> = regions
            .into_iter()
            .flat_map(|r| match r {
                Region::Sequence { regions } => regions,
                other => vec![other],
            })
            .filter(|r| !r.is_empty())
            .collect();

        match regions.len() {
            0 => Region::Sequence { regions: vec![] },
            1 => regions.into_iter().next().unwrap(),
            _ => Region::Sequence { regions },
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Region::Sequence { regions } => regions.is_empty(),
            Region::Parallel { branches } => branches.is_empty(),
            Region::Atomic { .. } => false,
        }
    }
}

#[derive(Debug)]
pub struct ControlFlowGraph {
    nodes: HashMap<usize, fk::Node>,
    edges: Vec<(usize, usize)>,
    labels: HashMap<String, usize>,
    label_at: HashMap<usize, String>,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            labels: HashMap::new(),
            label_at: HashMap::new(),
        }
    }

    pub fn from_graph(graph: &fk::Graph) -> Self {
        let mut cfg = ControlFlowGraph::new();

        for (idx, stmt) in graph.0.iter().enumerate() {
            if let Some(label) = &stmt.label {
                cfg.labels.insert(label.clone(), idx);
                cfg.label_at.insert(idx, label.clone());
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
        let region = self.build_from_index(0, &mut HashSet::new());
        let ir_node = Self::region_to_ir(&region);

        // If the top-level is a Seq, extract its children directly
        // to avoid an extra level of nesting
        match ir_node {
            ir::Node::Seq(children) => ir::Graph::new(children),
            other => ir::Graph::new(vec![other]),
        }
    }

    fn build_from_index(&self, start: usize, global_visited: &mut HashSet<usize>) -> Region {
        let mut regions = Vec::new();
        let mut current = start;

        loop {
            if global_visited.contains(&current) {
                break;
            }

            let Some(node) = self.nodes.get(&current) else {
                break;
            };

            match node {
                fk::Node::Atomic(name) => {
                    global_visited.insert(current);
                    regions.push(Region::Atomic { name: name.clone() });
                    current += 1;
                }
                fk::Node::Fork(target_label) => {
                    global_visited.insert(current);

                    // Find the join point - it's where all forked branches converge
                    let join_idx = self.find_join_for_fork(current);

                    // Collect all fork targets starting from this fork
                    let mut fork_targets = vec![current + 1]; // continuation (fall-through)
                    if let Some(&target_idx) = self.labels.get(target_label) {
                        fork_targets.push(target_idx);
                    }

                    // Check for consecutive forks
                    let mut check_idx = current + 1;
                    while let Some(fk::Node::Fork(next_target)) = self.nodes.get(&check_idx) {
                        global_visited.insert(check_idx);
                        if let Some(&target_idx) = self.labels.get(next_target) {
                            fork_targets.push(target_idx);
                        }
                        check_idx += 1;
                    }
                    // Update the continuation to be after all consecutive forks
                    fork_targets[0] = check_idx;

                    // Build each branch up to the join point
                    let mut branches = Vec::new();
                    for &branch_start in &fork_targets {
                        let branch_region =
                            self.build_branch_until(branch_start, join_idx, global_visited);
                        if !branch_region.is_empty() {
                            branches.push(branch_region);
                        }
                    }

                    if !branches.is_empty() {
                        regions.push(Region::Parallel { branches });
                    }

                    // Continue from after the join
                    if let Some(join_idx) = join_idx {
                        global_visited.insert(join_idx);
                        current = join_idx + 1;
                    } else {
                        break;
                    }
                }
                fk::Node::Join(_) => {
                    // Join encountered outside of fork processing - skip it
                    global_visited.insert(current);
                    current += 1;
                }
                fk::Node::Goto(target) => {
                    global_visited.insert(current);
                    if target == "end" {
                        break;
                    }
                    // Follow the goto
                    if let Some(&target_idx) = self.labels.get(target) {
                        if !global_visited.contains(&target_idx) {
                            current = target_idx;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        Region::sequence(regions)
    }

    fn build_branch_until(
        &self,
        start: usize,
        join_idx: Option<usize>,
        global_visited: &mut HashSet<usize>,
    ) -> Region {
        let mut regions = Vec::new();
        let mut current = start;

        loop {
            // Stop at join point
            if let Some(join) = join_idx {
                if current == join {
                    break;
                }
            }

            if global_visited.contains(&current) {
                break;
            }

            let Some(node) = self.nodes.get(&current) else {
                break;
            };

            match node {
                fk::Node::Atomic(name) => {
                    global_visited.insert(current);
                    regions.push(Region::Atomic { name: name.clone() });
                    current += 1;
                }
                fk::Node::Fork(target_label) => {
                    global_visited.insert(current);

                    // Nested fork - find its join
                    let nested_join_idx = self.find_join_for_fork(current);

                    let mut fork_targets = vec![current + 1];
                    if let Some(&target_idx) = self.labels.get(target_label) {
                        fork_targets.push(target_idx);
                    }

                    // Check for consecutive forks
                    let mut check_idx = current + 1;
                    while let Some(fk::Node::Fork(next_target)) = self.nodes.get(&check_idx) {
                        global_visited.insert(check_idx);
                        if let Some(&target_idx) = self.labels.get(next_target) {
                            fork_targets.push(target_idx);
                        }
                        check_idx += 1;
                    }
                    fork_targets[0] = check_idx;

                    let mut branches = Vec::new();
                    for &branch_start in &fork_targets {
                        let branch_region =
                            self.build_branch_until(branch_start, nested_join_idx, global_visited);
                        if !branch_region.is_empty() {
                            branches.push(branch_region);
                        }
                    }

                    if !branches.is_empty() {
                        regions.push(Region::Parallel { branches });
                    }

                    if let Some(nested_join) = nested_join_idx {
                        global_visited.insert(nested_join);
                        current = nested_join + 1;
                    } else {
                        break;
                    }
                }
                fk::Node::Join(_) => {
                    // This should be our target join or we've hit another join
                    break;
                }
                fk::Node::Goto(target) => {
                    global_visited.insert(current);
                    if target == "end" {
                        break;
                    }
                    // Check if goto leads to the join point
                    if let Some(&target_idx) = self.labels.get(target) {
                        if Some(target_idx) == join_idx {
                            break;
                        }
                        if global_visited.contains(&target_idx) {
                            break;
                        }
                        current = target_idx;
                    } else {
                        break;
                    }
                }
            }
        }

        Region::sequence(regions)
    }

    fn find_join_for_fork(&self, fork_idx: usize) -> Option<usize> {
        // Look for the next join statement after the fork
        // The join is typically where all branches converge

        // First, follow the main path to find a join
        let mut current = fork_idx + 1;
        let mut visited = HashSet::new();

        while let Some(node) = self.nodes.get(&current) {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current);

            match node {
                fk::Node::Join(_) => {
                    return Some(current);
                }
                fk::Node::Fork(_) => {
                    // Nested fork - skip to its join first
                    if let Some(nested_join) = self.find_join_for_fork(current) {
                        current = nested_join + 1;
                    } else {
                        current += 1;
                    }
                }
                fk::Node::Atomic(_) => {
                    current += 1;
                }
                fk::Node::Goto(target) => {
                    if target == "end" {
                        break;
                    }
                    if let Some(&target_idx) = self.labels.get(target) {
                        current = target_idx;
                    } else {
                        break;
                    }
                }
            }
        }

        None
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
                match ir_branches.len() {
                    0 => ir::Node::Par(vec![]),
                    1 => ir_branches.into_iter().next().unwrap(),
                    _ => ir::Node::Par(ir_branches),
                }
            }
        }
    }
}
