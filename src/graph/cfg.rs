use std::collections::{HashMap, HashSet};

use super::fk;
use super::ir;

#[derive(Debug, Clone)]
enum Region {
    Atomic {
        name: String,
        deps: Vec<String>,
        terminal: bool,
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

struct BuildCtx<'a> {
    join_labels: &'a HashMap<String, String>,
    dependency_join_labels: HashSet<String>,
    dependencies: &'a mut HashMap<String, Vec<String>>,
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
            println!("{idx} - {stmt:?}");
            cfg.nodes.insert(idx, stmt.node.clone());
        }

        for (idx, stmt) in graph.0.iter().enumerate() {
            match &stmt.node {
                fk::Node::Final => {
                    // No outgoing edges from final node
                }
                fk::Node::Goto { id: target_label } => {
                    if target_label == "_end" {
                        continue;
                    }
                    if let Some(&target_idx) = cfg.labels.get(target_label) {
                        cfg.edges.push((idx, target_idx));
                    }
                }
                fk::Node::Fork { id: target_label } => {
                    if let Some(&target_idx) = cfg.labels.get(target_label) {
                        cfg.edges.push((idx, target_idx));
                    } else if idx + 1 < graph.0.len() {
                        cfg.edges.push((idx, idx + 1));
                    }
                }
                fk::Node::Atomic { id } if id == "end" => {
                    if id == "end" {
                        continue;
                    }
                }
                fk::Node::Atomic { .. } | fk::Node::Join { .. } => {
                    if idx + 1 < graph.0.len() {
                        cfg.edges.push((idx, idx + 1));
                    }
                }
            }
        }

        cfg
    }

    /// Main function to map from Fork/Join to IR.
    pub fn to_ir(&self) -> ir::Graph {
        let join_labels = self.collect_join_labels();
        let dependency_join_labels = self.collect_dependency_join_labels(&join_labels);
        let mut dependencies = HashMap::new();
        let mut ctx = BuildCtx {
            join_labels: &join_labels,
            dependency_join_labels,
            dependencies: &mut dependencies,
        };
        let region = self.build_from_index(0, &mut HashSet::new(), &mut ctx);
        let region = Self::apply_dependencies(region, &dependencies);
        let ir_node = Self::region_to_ir(&region);

        // If the top-level is a Seq, extract its children directly
        // to avoid an extra level of nesting
        match ir_node {
            ir::Node::Seq(children) => ir::Graph::new(children),
            other => ir::Graph::new(vec![other]),
        }
    }

    fn collect_join_labels(&self) -> HashMap<String, String> {
        let mut join_labels = HashMap::new();
        let mut idx = 0;

        while self.nodes.contains_key(&idx) {
            if let Some(fk::Node::Join { .. }) = self.nodes.get(&idx) {
                if let Some(label) = self.label_at.get(&idx) {
                    let mut next = idx + 1;
                    while let Some(node) = self.nodes.get(&next) {
                        if let fk::Node::Atomic { id } = node {
                            join_labels.insert(label.clone(), id.clone());
                            break;
                        }
                        if matches!(node, fk::Node::Final) {
                            break;
                        }
                        next += 1;
                    }
                }
            }
            idx += 1;
        }

        join_labels
    }

    fn collect_dependency_join_labels(
        &self,
        join_labels: &HashMap<String, String>,
    ) -> HashSet<String> {
        let mut labels = HashSet::new();
        for node in self.nodes.values() {
            if let fk::Node::Fork { id } = node {
                if join_labels.contains_key(id) {
                    labels.insert(id.clone());
                }
            }
        }
        labels
    }

    fn build_from_index(
        &self,
        start: usize,
        global_visited: &mut HashSet<usize>,
        ctx: &mut BuildCtx<'_>,
    ) -> Region {
        let mut regions = Vec::new();
        let mut current = start;

        loop {
            if global_visited.contains(&current) {
                break;
            }

            let Some(node) = self.nodes.get(&current) else {
                break;
            };

            if self
                .label_at
                .get(&current)
                .is_some_and(|label| label == "_end")
            {
                break;
            }

            match node {
                fk::Node::Atomic { id } if id == "end" => {
                    global_visited.insert(current);
                    current += 1;
                }
                fk::Node::Atomic { id: name } => {
                    global_visited.insert(current);
                    let (dependents, terminal) = self.analyze_atomic(current, None, ctx);
                    Self::record_dependencies(ctx.dependencies, name, &dependents);
                    regions.push(Region::Atomic {
                        name: name.clone(),
                        deps: Vec::new(),
                        terminal,
                    });
                    current += 1;
                }
                fk::Node::Fork { id } if ctx.dependency_join_labels.contains(id) => {
                    global_visited.insert(current);
                    current += 1;
                }
                fk::Node::Fork { id: target_label } => {
                    global_visited.insert(current);

                    // Find the join point - it's where all forked branches converge
                    let join_idx = self.find_join_for_fork(current, ctx);

                    // Collect all fork targets starting from this fork
                    let mut fork_targets = vec![current + 1]; // continuation (fall-through)
                    if let Some(&target_idx) = self.labels.get(target_label) {
                        fork_targets.push(target_idx);
                    }

                    // Check for consecutive forks
                    let mut check_idx = current + 1;
                    while let Some(fk::Node::Fork { id: next_target }) = self.nodes.get(&check_idx)
                    {
                        global_visited.insert(check_idx);
                        if ctx.dependency_join_labels.contains(next_target) {
                            check_idx += 1;
                            continue;
                        }
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
                            self.build_branch_until(branch_start, join_idx, global_visited, ctx);
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
                fk::Node::Join { .. } => {
                    // Join encountered outside of fork processing - skip it
                    global_visited.insert(current);
                    current += 1;
                }
                fk::Node::Goto { id: target } => {
                    global_visited.insert(current);
                    if target == "end" {
                        break;
                    }
                    if ctx.dependency_join_labels.contains(target) {
                        break;
                    }
                    // Follow the goto
                    if let Some(&target_idx) = self.labels.get(target) {
                        if global_visited.contains(&target_idx) {
                            break;
                        }
                        current = target_idx;
                    } else {
                        break;
                    }
                }
                fk::Node::Final => {}
            }
        }

        Region::sequence(regions)
    }

    fn build_branch_until(
        &self,
        start: usize,
        join_idx: Option<usize>,
        global_visited: &mut HashSet<usize>,
        ctx: &mut BuildCtx<'_>,
    ) -> Region {
        let mut regions = Vec::new();
        let mut current = start;

        loop {
            // Stop at join point
            if let Some(join) = join_idx
                && current == join
            {
                break;
            }

            if global_visited.contains(&current) {
                break;
            }

            let Some(node) = self.nodes.get(&current) else {
                break;
            };

            if self
                .label_at
                .get(&current)
                .is_some_and(|label| label == "_end")
            {
                break;
            }

            match node {
                fk::Node::Atomic { id } if id == "end" => {
                    global_visited.insert(current);
                    current += 1;
                }
                fk::Node::Atomic { id: name } => {
                    global_visited.insert(current);
                    let (dependents, terminal) = self.analyze_atomic(current, join_idx, ctx);
                    Self::record_dependencies(ctx.dependencies, name, &dependents);
                    regions.push(Region::Atomic {
                        name: name.clone(),
                        deps: Vec::new(),
                        terminal,
                    });
                    current += 1;
                }
                fk::Node::Fork { id } if ctx.dependency_join_labels.contains(id) => {
                    global_visited.insert(current);
                    current += 1;
                }
                fk::Node::Fork { id: target_label } => {
                    global_visited.insert(current);

                    // Nested fork - find its join
                    let nested_join_idx = self.find_join_for_fork(current, ctx);

                    let mut fork_targets = vec![current + 1];
                    if let Some(&target_idx) = self.labels.get(target_label) {
                        fork_targets.push(target_idx);
                    }

                    // Check for consecutive forks
                    let mut check_idx = current + 1;
                    while let Some(fk::Node::Fork { id: next_target }) = self.nodes.get(&check_idx)
                    {
                        global_visited.insert(check_idx);
                        if ctx.dependency_join_labels.contains(next_target) {
                            check_idx += 1;
                            continue;
                        }
                        if let Some(&target_idx) = self.labels.get(next_target) {
                            fork_targets.push(target_idx);
                        }
                        check_idx += 1;
                    }
                    fork_targets[0] = check_idx;

                    let mut branches = Vec::new();
                    for &branch_start in &fork_targets {
                        let branch_region = self.build_branch_until(
                            branch_start,
                            nested_join_idx,
                            global_visited,
                            ctx,
                        );
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
                fk::Node::Join { .. } => {
                    // Skip dependency joins inside a branch
                    if let Some(label) = self.label_at.get(&current)
                        && ctx.dependency_join_labels.contains(label)
                    {
                        global_visited.insert(current);
                        current += 1;
                        continue;
                    }
                    // This should be our target join or we've hit another join
                    break;
                }
                fk::Node::Goto { id: target } => {
                    global_visited.insert(current);
                    if ctx.dependency_join_labels.contains(target) {
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
                fk::Node::Final => {}
            }
        }

        Region::sequence(regions)
    }

    fn analyze_atomic(
        &self,
        idx: usize,
        stop_join_idx: Option<usize>,
        ctx: &mut BuildCtx<'_>,
    ) -> (Vec<String>, bool) {
        let mut dependents = Vec::new();
        let mut terminal = false;
        let mut current = idx + 1;

        while let Some(node) = self.nodes.get(&current) {
            match node {
                fk::Node::Fork { id } => {
                    if ctx.join_labels.contains_key(id)
                        && let Some(dep) = ctx.join_labels.get(id)
                    {
                        ctx.dependency_join_labels.insert(id.clone());
                        dependents.push(dep.clone());
                        current += 1;
                        continue;
                    }
                    break;
                }
                fk::Node::Goto { id } => {
                    let mut is_structural_join = false;
                    if let Some(&target_idx) = self.labels.get(id)
                        && Some(target_idx) == stop_join_idx
                    {
                        is_structural_join = true;
                    }
                    if !is_structural_join && let Some(dep) = ctx.join_labels.get(id) {
                        ctx.dependency_join_labels.insert(id.clone());
                        dependents.push(dep.clone());
                        terminal = true;
                    }
                    break;
                }
                _ => break,
            }
        }

        dependents.sort();
        dependents.dedup();
        (dependents, terminal)
    }

    fn find_join_for_fork(&self, fork_idx: usize, ctx: &BuildCtx<'_>) -> Option<usize> {
        // Look for the next join statement after the fork
        // The join is typically where all branches converge

        // First, skip over any consecutive forks (they share the same join)
        let mut current = fork_idx + 1;
        while let Some(fk::Node::Fork { .. }) = self.nodes.get(&current) {
            current += 1;
        }

        let mut visited = HashSet::new();

        while let Some(node) = self.nodes.get(&current) {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current);

            match node {
                fk::Node::Join { .. } => {
                    if let Some(label) = self.label_at.get(&current)
                        && ctx.dependency_join_labels.contains(label)
                    {
                        current += 1;
                        continue;
                    }
                    return Some(current);
                }
                fk::Node::Fork { id } => {
                    if ctx.dependency_join_labels.contains(id) {
                        current += 1;
                        continue;
                    }
                    // Nested fork - skip to its join first
                    if let Some(nested_join) = self.find_join_for_fork(current, ctx) {
                        current = nested_join + 1;
                    } else {
                        current += 1;
                    }
                }
                fk::Node::Atomic { .. } | fk::Node::Final => {
                    current += 1;
                }
                fk::Node::Goto { id: target } => {
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

    fn apply_dependencies(region: Region, dependencies: &HashMap<String, Vec<String>>) -> Region {
        match region {
            Region::Atomic {
                name,
                deps: _,
                terminal,
            } => {
                let deps = dependencies.get(&name).cloned().unwrap_or_default();
                Region::Atomic {
                    name,
                    deps,
                    terminal,
                }
            }
            Region::Sequence { regions } => {
                let regions = regions
                    .into_iter()
                    .map(|region| Self::apply_dependencies(region, dependencies))
                    .collect();
                Region::sequence(regions)
            }
            Region::Parallel { branches } => Region::Parallel {
                branches: branches
                    .into_iter()
                    .map(|region| Self::apply_dependencies(region, dependencies))
                    .collect(),
            },
        }
    }

    fn record_dependencies(
        dependencies: &mut HashMap<String, Vec<String>>,
        dependency: &str,
        dependents: &[String],
    ) {
        for dependent in dependents {
            let entry = dependencies.entry(dependent.clone()).or_default();
            if !entry.iter().any(|dep| dep == dependency) {
                entry.push(dependency.to_string());
                entry.sort();
            }
        }
    }

    fn region_to_ir(region: &Region) -> ir::Node {
        match region {
            // here is so tuff, because we are not checking for the explicit dependencies and even we are not
            // checking if the node it's terminal
            // the pattern to detect that should be:
            // we are in a deffered branch (not the main one)
            // and the node (must be atomic) ends with `goto <join>` that mustn't go directly to `_end`.
            Region::Atomic {
                name,
                deps,
                terminal,
            } => ir::Node::Atomic(
                name.clone(),
                deps.iter().map(|dep| ir::Node::Dep(dep.clone())).collect(),
                *terminal,
            ),
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
