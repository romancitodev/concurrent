mod parser;
mod rendering;
use parser::grammar::parse;

use std::env::args;

fn main() {
    let input = args().nth(1).expect("Needed input");
    let graph = parse(input);
    rendering::render_graph(&graph.to_petgraph());
}
