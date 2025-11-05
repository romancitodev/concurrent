use std::collections::{HashMap, HashSet, VecDeque};

use crate::parser::fk::items::{Graph, Node};
use crate::parser::items as grammar;

/// Represents a region in the control flow graph
#[derive(Debug, Clone)]
enum Region {
    /// Sequential region containing a single atomic operation
    Atomic { idx: usize, name: String },
    /// Sequential region containing multiple sub-regions
    Sequence { regions: Vec<Region> },
    /// Parallel region with multiple branches that converge at a join
    Parallel {
        branches: Vec<Region>,
        join_idx: Option<usize>,
    },
}

#[derive(Debug)]
pub struct ControlFlowGraph {
    nodes: HashMap<usize, Node>,
    edges: Vec<(usize, usize)>, // (from_node_index, to_node_index)
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

        for (idx, stmt) in graph.0.iter().enumerate() {
            if let Some(label) = &stmt.label {
                cfg.labels.insert(label.clone(), idx);
            }
        }

        for (idx, stmt) in graph.0.iter().enumerate() {
            match &stmt.node {
                node @ Node::Goto(target_label) => {
                    cfg.nodes.insert(idx, node.clone());
                    // Special case: "goto end" is a terminal node (no edge created)
                    if target_label != "end"
                        && let Some(&target_idx) = cfg.labels.get(target_label)
                    {
                        cfg.edges.push((idx, target_idx));
                    }
                }
                Node::Fork(target_label) => {
                    // Fork creates two edges: one to the target label and one to the next statement
                    if let Some(&target_idx) = cfg.labels.get(target_label) {
                        cfg.edges.push((idx, target_idx));
                    }
                    // Add edge to continuation (next statement after fork)
                    if idx + 1 < graph.0.len() {
                        cfg.edges.push((idx, idx + 1));
                    }
                }
                Node::Atomic(_) | Node::Join => {
                    if idx + 1 < graph.0.len() {
                        cfg.edges.push((idx, idx + 1));
                    }
                }
                _ => {}
            }
            cfg.nodes.insert(idx, stmt.node.clone());
        }

        cfg
    }

    fn find_matching_join(&self, fork_idx: usize, targets: &[String]) -> Option<usize> {
        let expected_predecessors = 1 + targets.len();
        let mut candidates = HashSet::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(fork_idx);

        while let Some(idx) = queue.pop_front() {
            if visited.contains(&idx) {
                continue;
            }
            visited.insert(idx);

            if let Some(Node::Join) = self.nodes.get(&idx) {
                let preds = self.predecessors(idx);
                if preds.len() == expected_predecessors {
                    candidates.insert(idx);
                }
            }

            for &(from, to) in &self.edges {
                if from == idx {
                    queue.push_back(to);
                }
            }
        }

        candidates.into_iter().min()
    }

    fn predecessors(&self, idx: usize) -> Vec<usize> {
        self.edges
            .iter()
            .filter(|(_, to)| *to == idx)
            .map(|(from, _)| *from)
            .collect()
    }

    fn trace_path(&self, start: usize, end: usize) -> Vec<usize> {
        let mut path = Vec::new();
        let mut visited = HashSet::new();
        self.dfs_path(start, end, &mut path, &mut visited);
        path
    }

    fn dfs_path(
        &self,
        current: usize,
        end: usize,
        path: &mut Vec<usize>,
        visited: &mut HashSet<usize>,
    ) -> bool {
        if current == end {
            return true;
        }

        if visited.contains(&current) {
            return false;
        }

        visited.insert(current);

        if let Some(Node::Atomic(_)) = self.nodes.get(&current) {
            path.push(current);
        }

        for &(from, to) in &self.edges {
            if from == current && self.dfs_path(to, end, path, visited) {
                return true;
            }
        }

        if let Some(Node::Atomic(_)) = self.nodes.get(&current) {
            path.pop();
        }

        false
    }

    /// Converts the fork-join CFG to the grammar IR (parallel/sequential representation)
    pub fn to_ir(&self) -> grammar::Graph {
        // Use region-based analysis for better handling of complex CFGs
        let region = self.build_region_tree(0, &mut HashSet::new());
        let ir_nodes = self.region_to_ir(&region);
        grammar::Graph::new(vec![ir_nodes])
    }

    /// Builds a hierarchical region tree from the CFG
    fn build_region_tree(&self, start_idx: usize, visited: &mut HashSet<usize>) -> Region {
        if visited.contains(&start_idx) {
            return Region::Sequence { regions: vec![] };
        }

        if let Some(node) = self.nodes.get(&start_idx) {
            match node {
                Node::Fork(target_label) => {
                    visited.insert(start_idx);
                    self.build_fork_region(start_idx, target_label, visited)
                }
                Node::Atomic(name) => {
                    visited.insert(start_idx);
                    let atomic_region = Region::Atomic {
                        idx: start_idx,
                        name: name.clone(),
                    };

                    // Check if there's a next node
                    let next = self.find_successor(start_idx);
                    if let Some(next_idx) = next
                        && !visited.contains(&next_idx)
                    {
                        let next_region = self.build_region_tree(next_idx, visited);
                        match next_region {
                            Region::Sequence { regions } if regions.is_empty() => {
                                return atomic_region;
                            }
                            _ => {
                                return Region::Sequence {
                                    regions: vec![atomic_region, next_region],
                                };
                            }
                        }
                    }
                    atomic_region
                }
                Node::Join => {
                    visited.insert(start_idx);
                    // Continue after join
                    let next = self.find_successor(start_idx);
                    if let Some(next_idx) = next
                        && !visited.contains(&next_idx)
                    {
                        return self.build_region_tree(next_idx, visited);
                    }
                    Region::Sequence { regions: vec![] }
                }
                Node::Goto(target) => {
                    visited.insert(start_idx);
                    if target == "end" {
                        // Terminal goto
                        Region::Sequence { regions: vec![] }
                    } else {
                        // Follow the goto
                        let next = self.find_successor(start_idx);
                        if let Some(next_idx) = next
                            && !visited.contains(&next_idx)
                        {
                            return self.build_region_tree(next_idx, visited);
                        }
                        Region::Sequence { regions: vec![] }
                    }
                }
            }
        } else {
            Region::Sequence { regions: vec![] }
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

        // Find the join point
        let join_idx = if let Some(target) = target_idx {
            self.find_join_point(fork_idx, continuation_idx, Some(target))
        } else {
            None
        };

        let mut branches = Vec::new();

        // Build continuation branch (with its own visited set to allow independent exploration)
        let mut cont_visited = HashSet::new();
        let cont_region = if let Some(join) = join_idx {
            self.build_region_until_inclusive(continuation_idx, join, &mut cont_visited)
        } else {
            self.build_region_until_terminal(continuation_idx, &mut cont_visited)
        };
        let is_empty = matches!(&cont_region, Region::Sequence { regions } if regions.is_empty());
        if !is_empty {
            branches.push(cont_region);
        }
        // Merge cont_visited into global visited
        for idx in cont_visited {
            visited.insert(idx);
        }

        // Build target branch
        if let Some(target) = target_idx {
            let mut target_visited = HashSet::new();
            let target_region = if let Some(join) = join_idx {
                self.build_region_until_inclusive(target, join, &mut target_visited)
            } else {
                self.build_region_until_terminal(target, &mut target_visited)
            };
            let is_empty =
                matches!(&target_region, Region::Sequence { regions } if regions.is_empty());
            if !is_empty {
                branches.push(target_region);
            }
            // Merge target_visited into global visited
            for idx in target_visited {
                visited.insert(idx);
            }
        }

        let parallel = Region::Parallel { branches, join_idx };

        // Continue after join if exists
        if let Some(join) = join_idx {
            visited.insert(join);
            // Find the successor after the join via edges
            if let Some(next_idx) = self.find_successor(join)
                && !visited.contains(&next_idx)
            {
                let next_region = self.build_region_tree(next_idx, visited);
                match next_region {
                    Region::Sequence { regions } if regions.is_empty() => {
                        return parallel;
                    }
                    _ => {
                        return Region::Sequence {
                            regions: vec![parallel, next_region],
                        };
                    }
                }
            }
        }

        parallel
    }

    /// Builds a region from start until end (inclusive), then continues after the join
    fn build_region_until_inclusive(
        &self,
        start_idx: usize,
        end_idx: usize,
        visited: &mut HashSet<usize>,
    ) -> Region {
        let mut regions = Vec::new();
        let mut current = start_idx;

        // Build up to and including the end join
        while current != end_idx && !visited.contains(&current) {
            if let Some(node) = self.nodes.get(&current) {
                match node {
                    Node::Fork(target_label) => {
                        visited.insert(current);
                        let fork_region = self.build_fork_region(current, target_label, visited);
                        regions.push(fork_region);

                        // After processing fork, we might have jumped to join
                        // Check if we should continue
                        if visited.contains(&end_idx) {
                            break;
                        }

                        // Find next node via edges
                        if let Some(next) = self.find_successor(current) {
                            if next == end_idx || visited.contains(&next) {
                                break;
                            }
                            current = next;
                        } else {
                            break;
                        }
                    }
                    Node::Atomic(name) => {
                        visited.insert(current);
                        regions.push(Region::Atomic {
                            idx: current,
                            name: name.clone(),
                        });

                        // Move to next
                        if let Some(next) = self.find_successor(current) {
                            if next == end_idx {
                                break;
                            }
                            current = next;
                        } else {
                            break;
                        }
                    }
                    Node::Join => {
                        // Only stop if this is the target end join
                        if current == end_idx {
                            visited.insert(current);
                            break;
                        }
                        // Otherwise, continue through the join
                        visited.insert(current);
                        if let Some(next) = self.find_successor(current) {
                            if next == end_idx {
                                visited.insert(next);
                                break;
                            }
                            current = next;
                        } else {
                            break;
                        }
                    }
                    Node::Goto(_) => {
                        visited.insert(current);
                        // Follow goto
                        if let Some(next) = self.find_successor(current) {
                            if next == end_idx {
                                break;
                            }
                            current = next;
                        } else {
                            break;
                        }
                    }
                }
            } else {
                break;
            }
        }

        // After reaching the join, continue with what comes after
        if let Some(next_idx) = self.find_successor(end_idx) {
            if visited.contains(&next_idx) {
                if regions.is_empty() {
                    return Region::Sequence { regions: vec![] };
                } else if regions.len() == 1 {
                    return regions.into_iter().next().unwrap();
                }
            }
            let continuation = self.build_region_tree(next_idx, visited);
            match continuation {
                Region::Sequence {
                    regions: cont_regions,
                } if cont_regions.is_empty() => {
                    // No continuation, return as-is
                    if regions.is_empty() {
                        return Region::Sequence { regions: vec![] };
                    } else if regions.len() == 1 {
                        return regions.into_iter().next().unwrap();
                    }
                }
                _ => {
                    // Add continuation to regions
                    regions.push(continuation);
                    if regions.len() == 1 {
                        return regions.into_iter().next().unwrap();
                    }
                    return Region::Sequence { regions };
                }
            }
        }
        if regions.is_empty() {
            return Region::Sequence { regions: vec![] };
        } else if regions.len() == 1 {
            return regions.into_iter().next().unwrap();
        }
        Region::Sequence { regions }
    }

    /// Builds a region from start until a terminal node (goto end or no successor)
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

            if let Some(node) = self.nodes.get(&current) {
                match node {
                    Node::Fork(target_label) => {
                        visited.insert(current);
                        let fork_region = self.build_fork_region(current, target_label, visited);
                        regions.push(fork_region);
                        break; // Fork handles its own continuation
                    }
                    Node::Atomic(name) => {
                        visited.insert(current);
                        regions.push(Region::Atomic {
                            idx: current,
                            name: name.clone(),
                        });

                        if let Some(next) = self.find_successor(current) {
                            current = next;
                        } else {
                            break;
                        }
                    }
                    Node::Join => {
                        visited.insert(current);
                        if let Some(next) = self.find_successor(current) {
                            current = next;
                        } else {
                            break;
                        }
                    }
                    Node::Goto(_) => {
                        visited.insert(current);
                        // Goto is terminal
                        break;
                    }
                }
            } else {
                break;
            }
        }

        if regions.is_empty() {
            Region::Sequence { regions: vec![] }
        } else if regions.len() == 1 {
            regions.into_iter().next().unwrap()
        } else {
            Region::Sequence { regions }
        }
    }

    /// Converts a region tree to IR nodes
    fn region_to_ir(&self, region: &Region) -> grammar::Node {
        match region {
            Region::Atomic { name, .. } => grammar::Node::Atomic(name.clone(), vec![], false),
            Region::Sequence { regions } => {
                let ir_nodes: Vec<grammar::Node> =
                    regions.iter().map(|r| self.region_to_ir(r)).collect();

                if ir_nodes.is_empty() {
                    // This shouldn't happen, but handle it
                    grammar::Node::Seq(vec![])
                } else if ir_nodes.len() == 1 {
                    ir_nodes.into_iter().next().unwrap()
                } else {
                    grammar::Node::Seq(ir_nodes)
                }
            }
            Region::Parallel { branches, .. } => {
                let ir_branches: Vec<grammar::Node> =
                    branches.iter().map(|b| self.region_to_ir(b)).collect();

                grammar::Node::Par(ir_branches)
            }
        }
    }

    /// Finds the successor of a node in the CFG
    fn find_successor(&self, idx: usize) -> Option<usize> {
        for &(from, to) in &self.edges {
            if from == idx {
                return Some(to);
            }
        }
        None
    }

    fn convert_from_index(&self, idx: usize, visited: &mut HashSet<usize>) -> Vec<grammar::Node> {
        if visited.contains(&idx) {
            return vec![];
        }

        if let Some(node) = self.nodes.get(&idx) {
            match node {
                Node::Fork(target_label) => {
                    visited.insert(idx);

                    // Find fork branches
                    let continuation_idx = idx + 1;
                    let target_idx = self.labels.get(target_label).copied();

                    // Find the join point where both branches meet
                    if let Some(join_idx) = self.find_join_point(idx, continuation_idx, target_idx)
                    {
                        // Extract both parallel branches and mark all nodes in branches as visited
                        let mut branches = Vec::new();

                        // Branch 1: continuation (next statement after fork)
                        let (branch1, branch1_visited) =
                            self.extract_branch_with_visited(continuation_idx, join_idx);
                        for v in branch1_visited {
                            visited.insert(v);
                        }
                        if !branch1.is_empty() {
                            branches.push(if branch1.len() == 1 {
                                branch1.into_iter().next().unwrap()
                            } else {
                                grammar::Node::Seq(branch1)
                            });
                        }

                        // Branch 2: fork target
                        if let Some(target) = target_idx {
                            let (branch2, branch2_visited) =
                                self.extract_branch_with_visited(target, join_idx);
                            for v in branch2_visited {
                                visited.insert(v);
                            }
                            if !branch2.is_empty() {
                                branches.push(if branch2.len() == 1 {
                                    branch2.into_iter().next().unwrap()
                                } else {
                                    grammar::Node::Seq(branch2)
                                });
                            }
                        }

                        // Mark the join as visited too
                        visited.insert(join_idx);

                        let mut result = vec![grammar::Node::Par(branches)];

                        // Continue from after the join
                        result.extend(self.convert_from_index(join_idx + 1, visited));
                        return result;
                    }
                    // No join found - branches end without converging (e.g., both end with "goto end")
                    let mut branches = Vec::new();

                    // Branch 1: continuation (next statement after fork)
                    let (branch1, branch1_visited) =
                        self.extract_branch_to_terminal(continuation_idx);
                    for v in branch1_visited {
                        visited.insert(v);
                    }
                    if !branch1.is_empty() {
                        branches.push(if branch1.len() == 1 {
                            branch1.into_iter().next().unwrap()
                        } else {
                            grammar::Node::Seq(branch1)
                        });
                    }

                    // Branch 2: fork target
                    if let Some(target) = target_idx {
                        let (branch2, branch2_visited) = self.extract_branch_to_terminal(target);
                        for v in branch2_visited {
                            visited.insert(v);
                        }
                        if !branch2.is_empty() {
                            branches.push(if branch2.len() == 1 {
                                branch2.into_iter().next().unwrap()
                            } else {
                                grammar::Node::Seq(branch2)
                            });
                        }
                    }

                    // Return parallel branches without continuation
                    return vec![grammar::Node::Par(branches)];
                }
                Node::Atomic(name) => {
                    visited.insert(idx);
                    let mut result = vec![grammar::Node::Atomic(name.clone(), vec![], false)];

                    // Find next node
                    for &(from, to) in &self.edges {
                        if from == idx {
                            result.extend(self.convert_from_index(to, visited));
                            break;
                        }
                    }
                    return result;
                }
                Node::Join | Node::Goto(_) => {
                    visited.insert(idx);
                    // Skip these - they're structural
                    for &(from, to) in &self.edges {
                        if from == idx {
                            return self.convert_from_index(to, visited);
                        }
                    }
                }
            }
        }

        vec![]
    }

    fn find_join_point(
        &self,
        _fork_idx: usize,
        cont_idx: usize,
        target_idx: Option<usize>,
    ) -> Option<usize> {
        // Find where both branches converge
        let mut cont_reachable = HashSet::new();
        self.collect_reachable(cont_idx, &mut cont_reachable);

        if let Some(target) = target_idx {
            // Find first node reachable from target that is also in cont_reachable
            let mut target_path = Vec::new();
            self.collect_path(target, &mut target_path);

            for &idx in &target_path {
                if !cont_reachable.contains(&idx) {
                    continue;
                }
                if let Some(Node::Join) = self.nodes.get(&idx) {
                    return Some(idx);
                }
            }
        }

        None
    }

    fn collect_reachable(&self, start: usize, reachable: &mut HashSet<usize>) {
        if reachable.contains(&start) {
            return;
        }
        reachable.insert(start);

        for &(from, to) in &self.edges {
            if from == start {
                self.collect_reachable(to, reachable);
            }
        }
    }

    fn collect_path(&self, start: usize, path: &mut Vec<usize>) {
        if path.contains(&start) {
            return;
        }
        path.push(start);

        for &(from, to) in &self.edges {
            if from == start {
                self.collect_path(to, path);
            }
        }
    }

    fn extract_branch_with_visited(
        &self,
        start: usize,
        end: usize,
    ) -> (Vec<grammar::Node>, HashSet<usize>) {
        let mut result = Vec::new();
        let mut current = start;
        let mut visited = HashSet::new();

        while current != end && !visited.contains(&current) {
            visited.insert(current);

            if let Some(node) = self.nodes.get(&current) {
                match node {
                    Node::Atomic(name) => {
                        result.push(grammar::Node::Atomic(name.clone(), vec![], false));

                        // Move to next node
                        let mut found_next = false;
                        for &(from, to) in &self.edges {
                            if from == current {
                                current = to;
                                found_next = true;
                                break;
                            }
                        }
                        if !found_next {
                            break;
                        }
                    }
                    Node::Join => break,
                    Node::Goto(_) => {
                        // Follow the goto (or stop if it's "goto end")
                        let mut found_next = false;
                        for &(from, to) in &self.edges {
                            if from == current {
                                current = to;
                                found_next = true;
                                break;
                            }
                        }
                        if !found_next {
                            break;
                        }
                    }
                    Node::Fork(target_label) => {
                        // Handle nested fork recursively
                        let fork_continuation = current + 1;
                        let fork_target = self.labels.get(target_label).copied();

                        // Find join for this nested fork
                        if let Some(nested_join) =
                            self.find_join_point(current, fork_continuation, fork_target)
                        {
                            // Extract nested parallel branches
                            let mut nested_branches = Vec::new();

                            // Branch 1: continuation
                            let (branch1, visited1) =
                                self.extract_branch_with_visited(fork_continuation, nested_join);
                            for v in &visited1 {
                                visited.insert(*v);
                            }
                            if !branch1.is_empty() {
                                nested_branches.push(if branch1.len() == 1 {
                                    branch1.into_iter().next().unwrap()
                                } else {
                                    grammar::Node::Seq(branch1)
                                });
                            }

                            // Branch 2: target
                            if let Some(target) = fork_target {
                                let (branch2, visited2) =
                                    self.extract_branch_with_visited(target, nested_join);
                                for v in &visited2 {
                                    visited.insert(*v);
                                }
                                if !branch2.is_empty() {
                                    nested_branches.push(if branch2.len() == 1 {
                                        branch2.into_iter().next().unwrap()
                                    } else {
                                        grammar::Node::Seq(branch2)
                                    });
                                }
                            }

                            result.push(grammar::Node::Par(nested_branches));
                            visited.insert(nested_join);
                            current = nested_join + 1;
                        } else {
                            // No join found - extract terminal branches
                            let mut nested_branches = Vec::new();

                            let (branch1, visited1) =
                                self.extract_branch_to_terminal(fork_continuation);
                            for v in &visited1 {
                                visited.insert(*v);
                            }
                            if !branch1.is_empty() {
                                nested_branches.push(if branch1.len() == 1 {
                                    branch1.into_iter().next().unwrap()
                                } else {
                                    grammar::Node::Seq(branch1)
                                });
                            }

                            if let Some(target) = fork_target {
                                let (branch2, visited2) = self.extract_branch_to_terminal(target);
                                for v in &visited2 {
                                    visited.insert(*v);
                                }
                                if !branch2.is_empty() {
                                    nested_branches.push(if branch2.len() == 1 {
                                        branch2.into_iter().next().unwrap()
                                    } else {
                                        grammar::Node::Seq(branch2)
                                    });
                                }
                            }

                            result.push(grammar::Node::Par(nested_branches));
                            break; // Terminal branches, stop here
                        }
                    }
                }
            } else {
                break;
            }
        }

        (result, visited)
    }

    fn extract_branch_to_terminal(&self, start: usize) -> (Vec<grammar::Node>, HashSet<usize>) {
        let mut result = Vec::new();
        let mut current = start;
        let mut visited = HashSet::new();

        loop {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current);

            if let Some(node) = self.nodes.get(&current) {
                match node {
                    Node::Atomic(name) => {
                        result.push(grammar::Node::Atomic(name.clone(), vec![], false));

                        // Move to next node
                        let mut found_next = false;
                        for &(from, to) in &self.edges {
                            if from == current {
                                current = to;
                                found_next = true;
                                break;
                            }
                        }
                        if !found_next {
                            break;
                        }
                    }
                    Node::Join => break,
                    Node::Goto(_) => {
                        // Goto is terminal, stop here
                        break;
                    }
                    Node::Fork(target_label) => {
                        // Handle nested fork recursively
                        let fork_continuation = current + 1;
                        let fork_target = self.labels.get(target_label).copied();

                        let mut nested_branches = Vec::new();

                        // Extract both branches to terminal
                        let (branch1, visited1) =
                            self.extract_branch_to_terminal(fork_continuation);
                        for v in &visited1 {
                            visited.insert(*v);
                        }
                        if !branch1.is_empty() {
                            nested_branches.push(if branch1.len() == 1 {
                                branch1.into_iter().next().unwrap()
                            } else {
                                grammar::Node::Seq(branch1)
                            });
                        }

                        if let Some(target) = fork_target {
                            let (branch2, visited2) = self.extract_branch_to_terminal(target);
                            for v in &visited2 {
                                visited.insert(*v);
                            }
                            if !branch2.is_empty() {
                                nested_branches.push(if branch2.len() == 1 {
                                    branch2.into_iter().next().unwrap()
                                } else {
                                    grammar::Node::Seq(branch2)
                                });
                            }
                        }

                        result.push(grammar::Node::Par(nested_branches));
                        break; // After nested fork, this branch is done
                    }
                }
            } else {
                break;
            }
        }

        (result, visited)
    }
}
