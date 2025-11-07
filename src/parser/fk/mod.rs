//! Fork-Join grammar parser and control flow graph
//!
//! This module provides parsing and conversion utilities for fork-join programs.
//! Fork-join is a low-level control flow representation that uses explicit `fork`, `join`,
//! and `goto` statements to express parallelism.
//!
//! ## Example with Join
//!
//! ```text
//! begin
//!   a
//!   fork LB
//!   c
//!   LD: join
//!   d
//!   LB: b
//!   goto LD
//! end
//! ```
//!
//! This fork-join program executes:
//! 1. Task `a` sequentially
//! 2. Forks into two parallel branches: `c` and `b`
//! 3. Joins both branches at `LD`
//! 4. Executes task `d` after the join
//!
//! ## Example with Terminal Branches (goto end)
//!
//! ```text
//! begin
//!   s0
//!   s1
//!   fork L2
//!   s3
//!   s4
//!   goto end
//!   L2: s2
//!   s5
//!   goto end
//! end
//! ```
//!
//! When branches end with `goto end`, they are treated as terminal nodes
//! (no continuation after them). The IR conversion recognizes this pattern:
//! 1. Tasks `s0` and `s1` execute sequentially
//! 2. Fork into two parallel branches that terminate independently:
//!    - Branch 1: `s3` → `s4` → end
//!    - Branch 2: `s2` → `s5` → end
//!
//! The [`to_ir`] function converts this low-level representation into a high-level
//! parallel/sequential IR that is easier to analyze and visualize.

pub mod cfg;
pub mod grammar;
pub mod items;

use crate::parser::ir::items as ir;

/// Converts a fork-join graph to the grammar IR (parallel/sequential representation)
///
/// This function analyzes the control flow of a fork-join program and converts it into
/// a structured representation using parallel (`Par`) and sequential (`Seq`) nodes.
///
/// # Examples
///
/// ## Fork-Join with explicit join
///
/// ```no_run
/// use concurrent::parser::fk;
///
/// let input = "begin
///     a
///     fork LB
///     c
///     LD: join
///     d
///     LB: b
///     goto LD
///     end";
///
/// let fk_graph = fk::grammar::parse(input).unwrap();
/// let ir = fk::to_ir(&fk_graph);
///
/// // The IR will be:
/// // Seq([
/// //   Atomic("a"),
/// //   Par([
/// //     Atomic("c"),
/// //     Atomic("b")
/// //   ]),
/// //   Atomic("d")
/// // ])
/// ```
///
/// ## Fork with terminal branches (goto end)
///
/// ```no_run
/// use concurrent::parser::fk;
///
/// let input = "begin
///     s0
///     fork L2
///     s3
///     goto end
///     L2: s2
///     goto end
///     end";
///
/// let fk_graph = fk::grammar::parse(input).unwrap();
/// let ir = fk::to_ir(&fk_graph);
///
/// // The IR will be:
/// // Seq([
/// //   Atomic("s0"),
/// //   Par([
/// //     Atomic("s3"),
/// //     Atomic("s2")
/// //   ])
/// // ])
/// // Note: Both branches terminate with "goto end" so there's no join
/// ```
///
/// # Arguments
///
/// * `graph` - A fork-join graph parsed from source code
///
/// # Returns
///
/// A grammar IR graph with parallel and sequential structure
pub fn to_ir(graph: &items::Graph) -> ir::Graph {
    let cfg = cfg::ControlFlowGraph::from_graph(graph);
    cfg.to_ir()
}

impl From<ir::Graph> for items::Graph {
    fn from(graph: ir::Graph) -> Self {
        let cfg = cfg::ControlFlowGraph::from_graph(&graph);
        cfg.to_ir()
    }
}

#[expect(unused)]
pub fn from_ir(graph: &ir::Graph) -> items::Graph {
    todo!()
}
