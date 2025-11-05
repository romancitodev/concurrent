mod parser;
mod rendering;
mod validator;
use parser::grammar::parse;
use validator::validate;

use std::{env::args, path::Path};

fn main() {
    let input = args().nth(1).expect("Needed input");
    let path = args().nth(2).unwrap_or_else(|| "render/output".to_string());

    let path = Path::new(&path);
    let graph = parse(input);

    // Validate the graph
    if let Err(errors) = validate(&graph) {
        eprintln!("❌ Validation errors found:\n");
        for error in errors {
            eprintln!("  • {}", error.message);
        }
        std::process::exit(1);
    }

    let svg = rendering::render_to_svg(&graph.to_petgraph());
    rendering::render_svg_to_pdf(svg, path).unwrap();
}

// begin
// s1
// fork L2
// s3
// L4: join C1
// s4
// goto final
// L2: s2
// goto L4
// final: end
