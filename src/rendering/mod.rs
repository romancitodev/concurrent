pub mod pdf;
use petgraph::dot::{Config, Dot};
use std::{fmt::Write, io, path::Path};

use layout::{backends::svg::SVGWriter, gv, topo::layout::VisualGraph};

pub fn render_graph(graph: &pdf::Flow) -> String {
    let mut buffer = String::new();
    write!(
        &mut buffer,
        "{}",
        Dot::with_config(graph, &[Config::EdgeNoLabel])
    )
    .unwrap();
    buffer
}

pub fn render_to_svg(graph: &pdf::Flow) -> String {
    let dot_string = render_graph(graph);
    let mut parser = gv::DotParser::new(&dot_string);

    let tree = parser.process().expect("Unable to parse the file");
    let mut gb = gv::GraphBuilder::new();
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
