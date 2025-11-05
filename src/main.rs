mod parser;
mod rendering;
mod validator;
use parser::grammar::parse;
use validator::validate;

use std::{env::args, path::Path};

fn main() {
    let input = args().nth(1).expect("Needed input");
    let path = args().nth(2).unwrap_or_else(|| "render/output".to_string());
    let type_ = args().nth(3).unwrap_or_else(|| "grammar".to_string());

    let path = Path::new(&path);

    match type_.as_str() {
        "grammar" => {
            let graph = parse(&input);

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
        "f/k" => {
            use parser::fk::cfg::ControlFlowGraph;
            use parser::fk::grammar::parse;
            let fk_graph = parse(&input).unwrap();

            let cfg = ControlFlowGraph::from_graph(&fk_graph);

            let ir_graph = parser::fk::to_ir(&fk_graph);
            let svg = rendering::render_to_svg(&ir_graph.to_petgraph());
            rendering::render_svg_to_pdf(svg, path).unwrap();
        }
        _ => {
            eprintln!("Unknown type: {type_:?}");
            std::process::exit(1);
        }
    }
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
