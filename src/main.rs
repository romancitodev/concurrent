mod parser;
mod rendering;
use parser::grammar::parse;

use std::{env::args, path::Path};

fn main() {
    let input = args().nth(1).expect("Needed input");
    let path = args()
        .nth(2)
        .unwrap_or_else(|| "render/output.svg".to_string());

    let path = Path::new(&path);
    let graph = parse(input);
    let buffer = rendering::render_graph(&graph.to_petgraph());
    println!("{buffer}");
    let svg = String::from_utf8(rendering::render_to_svg(&graph.to_petgraph())).unwrap();
    rendering::render_svg_to_pdf(svg, path).unwrap();
}
