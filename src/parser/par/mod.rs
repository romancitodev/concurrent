pub mod grammar;
pub mod items;

use crate::parser::ir::items as ir;

pub fn to_ir(graph: &items::Graph) -> ir::Graph {
    graph.to_ir()
}

// Maybe it's way convinient to use From<> instead of creating a method.
#[expect(unused)]
pub fn from_ir(graph: &ir::Graph) -> items::Graph {
    todo!()
}
