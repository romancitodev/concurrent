mod error;
mod graph;
mod render;
mod validate;

use std::path::{Path, PathBuf};

pub use error::{Error, ValidationError, ValidationErrorKind};
pub use graph::{ForkJoin, Graph, Ir, IrNode, Par, Unvalidated, Valid};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ir,
    Par,
    ForkJoin,
}

pub struct ValidatedGraph {
    petgraph: petgraph::Graph<String, &'static str>,
}

impl ValidatedGraph {
    #[must_use]
    pub fn render_to_svg(&self) -> String {
        render::render_to_svg(&self.petgraph)
    }

    #[must_use]
    pub fn petgraph(&self) -> &petgraph::Graph<String, &'static str> {
        &self.petgraph
    }

    #[must_use]
    pub fn into_petgraph(self) -> petgraph::Graph<String, &'static str> {
        self.petgraph
    }
}

pub fn parse_and_validate(input: &str, format: Format) -> Result<ValidatedGraph, Error> {
    let graph = parse(input, format)?;

    let validated = graph.validate()?;
    Ok(ValidatedGraph {
        petgraph: validated.to_petgraph(),
    })
}

pub fn parse(input: &str, format: Format) -> Result<Graph<IrNode, Ir, Unvalidated>, Error> {
    let ir = match format {
        Format::Ir => Graph::<IrNode, Ir>::parse(input)?,
        Format::Par => Graph::<graph::par::Node, Par>::parse(input)?.to_ir(),
        Format::ForkJoin => Graph::<graph::fk::Stmt, ForkJoin>::parse(input)?.to_ir(),
    };

    Ok(ir)
}

pub fn render_to_pdf(svg: &str, path: &std::path::Path) -> Result<(), Error> {
    render::render_svg_to_pdf(svg, path)
        .map_err(|e| Error::RenderError(format!("Failed to render PDF: {e}")))
}

pub fn process_graph_to_pdf(
    input: &str,
    output_path: &std::path::Path,
    ext: &str,
) -> Result<(), Error> {
    let format = format_from_ext(ext)?;
    let graph = parse_and_validate(input, format)?;
    let svg = graph.render_to_svg();
    render_to_pdf(&svg, output_path)
}

pub fn process_graph_to_ir(
    input: &str,
    output_path: &std::path::Path,
    ext: &str,
) -> Result<(), Error> {
    let format = format_from_ext(ext)?;

    let ir = match format {
        Format::Ir => Graph::<IrNode, Ir>::parse(input)?,
        Format::Par => Graph::<graph::par::Node, Par>::parse(input)?.to_ir(),
        Format::ForkJoin => Graph::<graph::fk::Stmt, ForkJoin>::parse(input)?.to_ir(),
    };

    std::fs::write(output_path, format!("{ir}"))
        .map_err(|e| Error::RenderError(format!("Failed to write IR: {e}")))?;

    Ok(())
}

fn format_from_ext(ext: &str) -> Result<Format, Error> {
    match ext {
        "graph" => Ok(Format::Ir),
        "par" => Ok(Format::Par),
        "fk" => Ok(Format::ForkJoin),
        _ => Err(Error::InvalidType(ext.to_string())),
    }
}

pub fn convert_graph(input: &str, output: &Path, ex: &str) -> Result<(), Error> {
    let format = format_from_ext(ex)?;
    let graph = parse(input, format)?;

    let output_ext = output
        .extension()
        .expect("Output file must have an extension")
        .to_str()
        .unwrap();

    let format_ext = format_from_ext(output_ext)?;

    let graph = match format_ext {
        Format::Ir => graph.to_string(),
        Format::Par => graph.to_par().to_string(),
        Format::ForkJoin => graph.to_fk().to_string(),
    };

    std::fs::write(output, graph)
        .map_err(|e| Error::RenderError(format!("Failed to write IR: {e}")))?;

    Ok(())
}
