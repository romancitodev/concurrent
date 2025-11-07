mod parser;
mod rendering;
mod validator;
use parser::ir::grammar::parse;
use validator::validate;

use std::{env::args, path::Path};

fn main() {
    let input = args().nth(1).expect("Needed input file");
    let path = args().nth(2).unwrap_or_else(|| "render/output".to_string());
    let type_ = args().nth(3).unwrap_or_else(|| "grammar".to_string());

    let input = std::fs::read_to_string(&input).expect("Failed to read input file");
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
        "par" => {
            use parser::par::grammar::parse;
            let par_graph = parse(&input).unwrap();

            println!("{par_graph:#?}");

            let ir = parser::par::to_ir(&par_graph);
            let svg = rendering::render_to_svg(&ir.to_petgraph());

            rendering::render_svg_to_pdf(svg, path).unwrap();
        }
        "f/j" => {
            use parser::fk::grammar::parse;
            let fk_graph = parse(&input).unwrap();

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
