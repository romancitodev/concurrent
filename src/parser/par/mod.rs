pub mod grammar;
pub mod items;

use crate::parser::items as grammar_items;

pub fn to_ir(graph: &items::Graph) -> grammar_items::Graph {
    graph.to_ir()
}
