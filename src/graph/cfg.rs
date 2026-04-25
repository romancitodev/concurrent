use std::collections::{HashMap, HashSet};

use super::fk;
use super::ir;

#[derive(Debug, Clone)]
enum Region {
    Atomic {
        name: String,
        deps: Vec<String>,
        is_terminal: bool,
    },
    Sequence {
        regions: Vec<Region>,
    },
    Parallel {
        branches: Vec<Region>,
    },
}

impl Region {
    fn sequence(regions: Vec<Region>) -> Self {
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

    // For dependencies inference
    signaled_by: HashMap<String, Vec<String>>, // join_label -> Vec<atomic_name>
    waits_for: HashMap<usize, String>,         // atomic_idx -> join_label
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            labels: HashMap::new(),
            label_at: HashMap::new(),
            signaled_by: HashMap::new(),
            waits_for: HashMap::new(),
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

        cfg.build_edges_and_waits(graph);
        cfg.find_signalers(graph);

        cfg
    }

    fn build_edges_and_waits(&mut self, graph: &fk::Graph) {
        let mut current_join_label = None;
        for (idx, stmt) in graph.0.iter().enumerate() {
            match &stmt.node {
                fk::Node::Goto(target_label) => {
                    if target_label != "end"
                        && target_label != "_end"
                        && let Some(&target_idx) = self.labels.get(target_label)
                    {
                        self.edges.push((idx, target_idx));
                    }
                }
                fk::Node::Fork(target_label) => {
                    if let Some(&target_idx) = self.labels.get(target_label) {
                        self.edges.push((idx, target_idx));
                    }
                    if idx + 1 < graph.0.len() {
                        self.edges.push((idx, idx + 1));
                    }
                }
                fk::Node::Join(_) => {
                    if let Some(label) = &stmt.label {
                        current_join_label = Some(label.clone());
                    }
                    if idx + 1 < graph.0.len() {
                        self.edges.push((idx, idx + 1));
                    }
                }
                fk::Node::Atomic(name) => {
                    if name != "end" && name != "_end" {
                        if let Some(lbl) = current_join_label.take() {
                            self.waits_for.insert(idx, lbl);
                        }
                    }
                    if idx + 1 < graph.0.len() {
                        self.edges.push((idx, idx + 1));
                    }
                }
            }
        }
    }

    fn find_signalers(&mut self, graph: &fk::Graph) {
        for (idx, stmt) in graph.0.iter().enumerate() {
            if let fk::Node::Atomic(name) = &stmt.node {
                if name == "end" || name == "_end" {
                    continue;
                }

                let mut look_idx = idx + 1;
                while let Some(node) = self.nodes.get(&look_idx) {
                    match node {
                        fk::Node::Fork(target) | fk::Node::Goto(target) => {
                            let is_join = if let Some(&t_idx) = self.labels.get(target) {
                                matches!(self.nodes.get(&t_idx), Some(fk::Node::Join(_)))
                            } else {
                                false
                            };

                            if is_join {
                                self.signaled_by
                                    .entry(target.clone())
                                    .or_default()
                                    .push(name.clone());
                            }
                            if matches!(node, fk::Node::Goto(_)) {
                                break;
                            }
                            look_idx += 1;
                        }
                        _ => break,
                    }
                }
            }
        }
    }

    pub fn to_ir(&self) -> ir::Graph {
        let region = self.build_from_index(0, &mut HashSet::new());
        let ir_node = Self::region_to_ir(&region);

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
                    if name == "end" || name == "_end" {
                        break;
                    }

                    let mut deps = Vec::new();
                    if let Some(join_lbl) = self.waits_for.get(&current) {
                        if let Some(signalers) = self.signaled_by.get(join_lbl) {
                            deps = signalers.clone();
                        }
                    }

                    // Check if it's terminal
                    let mut is_terminal = false;
                    let mut look_idx = current + 1;
                    while let Some(n) = self.nodes.get(&look_idx) {
                        match n {
                            fk::Node::Fork(_) => {
                                look_idx += 1;
                            }
                            fk::Node::Goto(tgt) => {
                                if tgt == "end" || tgt == "_end" {
                                    is_terminal = true;
                                }
                                break;
                            }
                            _ => break,
                        }
                    }

                    regions.push(Region::Atomic {
                        name: name.clone(),
                        deps,
                        is_terminal,
                    });
                    current += 1;
                }
                fk::Node::Fork(target_label) => {
                    global_visited.insert(current);

                    // Skip if this fork is just signaling a dependency
                    if self.signaled_by.contains_key(target_label) {
                        current += 1;
                        continue;
                    }

                    let join_idx = self.find_join_for_fork(current);

                    let mut fork_targets = vec![current + 1];
                    if let Some(&target_idx) = self.labels.get(target_label) {
                        fork_targets.push(target_idx);
                    }

                    let mut check_idx = current + 1;
                    while let Some(fk::Node::Fork(next_target)) = self.nodes.get(&check_idx) {
                        global_visited.insert(check_idx);
                        if !self.signaled_by.contains_key(next_target) {
                            if let Some(&target_idx) = self.labels.get(next_target) {
                                fork_targets.push(target_idx);
                            }
                        }
                        check_idx += 1;
                    }
                    fork_targets[0] = check_idx;

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

                    if let Some(join_idx) = join_idx {
                        global_visited.insert(join_idx);
                        current = join_idx + 1;
                    } else {
                        // Just fallthrough if no join was identified
                        current = fork_targets[0];
                    }
                }
                fk::Node::Join(_) => {
                    global_visited.insert(current);
                    current += 1;
                }
                fk::Node::Goto(target) => {
                    global_visited.insert(current);
                    if target == "end" || target == "_end" {
                        break;
                    }

                    if self.signaled_by.contains_key(target) {
                        break; // Just a dependency signal
                    }

                    if let Some(&target_idx) = self.labels.get(target) {
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

    fn build_branch_until(
        &self,
        start: usize,
        join_idx: Option<usize>,
        global_visited: &mut HashSet<usize>,
    ) -> Region {
        let mut regions = Vec::new();
        let mut current = start;

        loop {
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
                    if name == "end" || name == "_end" {
                        break;
                    }

                    let mut deps = Vec::new();
                    if let Some(join_lbl) = self.waits_for.get(&current) {
                        if let Some(signalers) = self.signaled_by.get(join_lbl) {
                            deps = signalers.clone();
                        }
                    }

                    let mut is_terminal = false;
                    let mut look_idx = current + 1;
                    while let Some(n) = self.nodes.get(&look_idx) {
                        match n {
                            fk::Node::Fork(_) => {
                                look_idx += 1;
                            }
                            fk::Node::Goto(tgt) => {
                                if tgt == "end" || tgt == "_end" {
                                    is_terminal = true;
                                }
                                break;
                            }
                            _ => break,
                        }
                    }

                    regions.push(Region::Atomic {
                        name: name.clone(),
                        deps,
                        is_terminal,
                    });
                    current += 1;
                }
                fk::Node::Fork(target_label) => {
                    global_visited.insert(current);

                    if self.signaled_by.contains_key(target_label) {
                        current += 1;
                        continue;
                    }

                    let nested_join_idx = self.find_join_for_fork(current);

                    let mut fork_targets = vec![current + 1];
                    if let Some(&target_idx) = self.labels.get(target_label) {
                        fork_targets.push(target_idx);
                    }

                    let mut check_idx = current + 1;
                    while let Some(fk::Node::Fork(next_target)) = self.nodes.get(&check_idx) {
                        global_visited.insert(check_idx);
                        if !self.signaled_by.contains_key(next_target) {
                            if let Some(&target_idx) = self.labels.get(next_target) {
                                fork_targets.push(target_idx);
                            }
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
                    if Some(current) == join_idx {
                        break;
                    }
                    global_visited.insert(current);
                    current += 1;
                }
                fk::Node::Goto(target) => {
                    global_visited.insert(current);
                    if target == "end" || target == "_end" {
                        break;
                    }

                    if self.signaled_by.contains_key(target) {
                        break;
                    }

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
        let mut current = fork_idx + 1;
        while let Some(fk::Node::Fork(tgt)) = self.nodes.get(&current) {
            if !self.signaled_by.contains_key(tgt) {
                current += 1;
            } else {
                break;
            }
        }

        let mut visited = HashSet::new();

        while let Some(node) = self.nodes.get(&current) {
            if !visited.insert(current) {
                break;
            }

            match node {
                fk::Node::Join(_) => {
                    // Si el join actual esta referenciado como senal, lo omitimos
                    if let Some(lbl) = self.label_at.get(&current) {
                        if self.signaled_by.contains_key(lbl) {
                            current += 1;
                            continue;
                        }
                    }
                    return Some(current);
                }
                fk::Node::Fork(tgt) => {
                    if self.signaled_by.contains_key(tgt) {
                        current += 1;
                        continue;
                    }
                    if let Some(nested_join) = self.find_join_for_fork(current) {
                        current = nested_join + 1;
                    } else {
                        current += 1;
                    }
                }
                fk::Node::Atomic(name) => {
                    if name == "end" || name == "_end" {
                        break;
                    }
                    current += 1;
                }
                fk::Node::Goto(target) => {
                    if target == "end" || target == "_end" {
                        break;
                    }
                    if self.signaled_by.contains_key(target) {
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
            Region::Atomic {
                name,
                deps,
                is_terminal,
            } => {
                let dep_nodes = deps.iter().map(|d| ir::Node::Dep(d.clone())).collect();
                ir::Node::Atomic(name.clone(), dep_nodes, *is_terminal)
            }
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
