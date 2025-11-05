pub mod pdf;
use std::{fmt::Write, io, path::Path};

use graphviz_rust::{cmd::Format, parse, printer::PrinterContext};
use petgraph::dot::{Config, Dot};

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

pub fn render_to_svg(graph: &pdf::Flow) -> Vec<u8> {
    let dot_string = render_graph(graph);
    graphviz_rust::exec(
        parse(&dot_string).expect("can't parse dot string"),
        &mut PrinterContext::default(),
        vec![Format::Svg.into()],
    )
    .expect("Failed to render SVG")
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
