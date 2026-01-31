use std::fmt::Write;
use std::io;
use std::path::Path;

use layout::backends::svg::SVGWriter;
use layout::gv::{DotParser, GraphBuilder};
use layout::topo::layout::VisualGraph;
use petgraph::Directed;
use petgraph::dot::{Config, Dot};
use petgraph::graph::Graph as PetGraph;

pub type Flow = PetGraph<String, &'static str, Directed>;

pub fn render_graph(graph: &Flow) -> String {
    let mut buffer = String::new();
    write!(
        &mut buffer,
        "{}",
        Dot::with_config(graph, &[Config::EdgeNoLabel])
    )
    .unwrap();
    buffer
}

pub fn render_to_svg(graph: &Flow) -> String {
    let dot_string = render_graph(graph);
    let mut parser = DotParser::new(&dot_string);

    let tree = parser.process().expect("Unable to parse the file");
    let mut gb = GraphBuilder::new();
    gb.visit_graph(&tree);
    let mut visual_graph = gb.get();
    generate_svg(&mut visual_graph)
}

fn generate_svg(graph: &mut VisualGraph) -> String {
    let mut svg = SVGWriter::new();
    graph.do_it(false, false, false, &mut svg);
    svg.finalize()
}

pub fn render_svg_to_pdf(svg: impl AsRef<str>, output: &Path) -> io::Result<()> {
    use svg2pdf::{ConversionOptions, PageOptions};

    let output = output.with_extension("pdf");

    let mut options = svg2pdf::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(svg.as_ref(), &options).unwrap();

    let pdf = svg2pdf::to_pdf(&tree, ConversionOptions::default(), PageOptions::default()).unwrap();
    std::fs::write(output, pdf)?;
    Ok(())
}
