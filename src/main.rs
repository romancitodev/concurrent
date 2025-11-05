mod parser;
mod rendering;
use parser::grammar::parse;

use std::{env::args, path::Path};

fn main() {
    let input = args().nth(1).expect("Needed input");
    let path = args().nth(2).unwrap_or_else(|| "render/output".to_string());

    let path = Path::new(&path);
    let graph = parse(input);
    let svg = rendering::render_to_svg(&graph.to_petgraph());
    rendering::render_svg_to_pdf(svg, path).unwrap();
}
