mod error;
mod graph;
mod render;
mod validate;

#[cfg(feature = "cli")]
use std::path::Path;

#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

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

#[cfg_attr(feature = "web", derive(serde::Serialize))]
pub struct GraphNodeData<'a> {
    pub id: &'a str,
}

#[cfg_attr(feature = "web", derive(serde::Serialize))]
pub struct GraphEdgeData<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub kind: &'static str,
}

#[cfg_attr(feature = "web", derive(serde::Serialize))]
pub struct GraphData<'a> {
    pub nodes: Vec<GraphNodeData<'a>>,
    pub edges: Vec<GraphEdgeData<'a>>,
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

    #[must_use]
    pub fn data(&self) -> GraphData<'_> {
        use petgraph::visit::EdgeRef;

        let nodes = self
            .petgraph
            .node_weights()
            .map(|id| GraphNodeData { id: id.as_str() })
            .collect();

        let edges = self
            .petgraph
            .edge_references()
            .map(|edge| {
                let from = self.petgraph[edge.source()].as_str();
                let to = self.petgraph[edge.target()].as_str();
                let kind = if edge.weight().is_empty() {
                    "flow"
                } else {
                    *edge.weight()
                };
                GraphEdgeData { from, to, kind }
            })
            .collect();

        GraphData { nodes, edges }
    }
}

pub fn parse_and_validate(input: &str, format: Format) -> Result<ValidatedGraph, Error> {
    let graph = parse(input, format)?;

    let validated = graph.validate()?;
    Ok(ValidatedGraph {
        petgraph: validated.to_petgraph(),
    })
}

pub fn render_graph_to_svg(input: &str, format: Format) -> Result<String, Error> {
    let graph = parse_and_validate(input, format)?;
    Ok(graph.render_to_svg())
}

pub fn convert_graph_string(
    input: &str,
    input_format: Format,
    output_format: Format,
) -> Result<String, Error> {
    let graph = parse(input, input_format)?;

    let output = match output_format {
        Format::Ir => graph.to_string(),
        Format::Par => graph.to_par()?.to_string(),
        Format::ForkJoin => graph.to_fk().to_string(),
    };

    Ok(output)
}

pub fn parse(input: &str, format: Format) -> Result<Graph<IrNode, Ir, Unvalidated>, Error> {
    let ir = match format {
        Format::Ir => Graph::<IrNode, Ir>::parse(input)?,
        Format::Par => Graph::<graph::par::Node, Par>::parse(input)?.to_ir(),
        Format::ForkJoin => Graph::<graph::fk::Stmt, ForkJoin>::parse(input)?.to_ir(),
    };

    Ok(ir)
}

#[cfg(feature = "cli")]
pub fn render_to_pdf(svg: &str, path: &Path) -> Result<(), Error> {
    render::render_svg_to_pdf(svg, path)
        .map_err(|e| Error::RenderError(format!("Failed to render PDF: {e}")))
}

#[cfg(feature = "cli")]
pub fn process_graph_to_pdf(input: &str, output_path: &Path, ext: &str) -> Result<(), Error> {
    let format = ext.try_into()?;
    let graph = parse_and_validate(input, format)?;
    let svg = graph.render_to_svg();
    render_to_pdf(&svg, output_path)
}

#[cfg(feature = "cli")]
pub fn process_graph_to_ir(input: &str, output_path: &Path, ext: &str) -> Result<(), Error> {
    let format = ext.try_into()?;

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

#[cfg(feature = "web")]
fn format_from_web(format: &str) -> Result<Format, Error> {
    let normalized = format.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "ir" | "graph" => Ok(Format::Ir),
        "par" | "parbegin" | "parbegin/parend" | "parbegin_parend" | "parbegin-parend" => {
            Ok(Format::Par)
        }
        "fk" | "fork_join" | "fork-join" | "fork/join" => Ok(Format::ForkJoin),
        _ => Err(Error::InvalidType(format.to_string())),
    }
}

#[cfg(feature = "web")]
fn map_wasm_error(error: Error) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[cfg(feature = "web")]
#[wasm_bindgen]
pub fn convert_graph_string_wasm(
    input: &str,
    input_format: &str,
    output_format: &str,
) -> Result<String, JsValue> {
    let input_format = format_from_web(input_format).map_err(map_wasm_error)?;
    let output_format = format_from_web(output_format).map_err(map_wasm_error)?;

    convert_graph_string(input, input_format, output_format).map_err(map_wasm_error)
}

#[cfg(feature = "web")]
#[wasm_bindgen]
pub fn render_graph_to_svg_wasm(input: &str, input_format: &str) -> Result<String, JsValue> {
    let format = format_from_web(input_format).map_err(map_wasm_error)?;
    render_graph_to_svg(input, format).map_err(map_wasm_error)
}

#[cfg(feature = "web")]
#[wasm_bindgen]
pub fn graph_data_wasm(input: &str, input_format: &str) -> Result<JsValue, JsValue> {
    let format = format_from_web(input_format).map_err(map_wasm_error)?;
    let graph = parse_and_validate(input, format).map_err(map_wasm_error)?;
    serde_wasm_bindgen::to_value(&graph.data())
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

impl TryFrom<&str> for Format {
    type Error = Error;

    fn try_from(ext: &str) -> Result<Self, Self::Error> {
        match ext {
            "graph" => Ok(Format::Ir),
            "par" => Ok(Format::Par),
            "fk" => Ok(Format::ForkJoin),
            _ => Err(Error::InvalidType(ext.to_string())),
        }
    }
}

#[cfg(feature = "cli")]
pub fn convert_graph(input: &str, output: &Path, ex: &str) -> Result<(), Error> {
    let format = ex.try_into()?;
    let graph = parse(input, format)?;

    let output_ext = output
        .extension()
        .expect("Output file must have an extension")
        .to_str()
        .unwrap();

    let format_ext = output_ext.try_into()?;

    let graph = match format_ext {
        Format::Ir => graph.to_string(),
        Format::Par => graph.to_par()?.to_string(),
        Format::ForkJoin => graph.to_fk().to_string(),
    };

    std::fs::write(output, graph)
        .map_err(|e| Error::RenderError(format!("Failed to write IR: {e}")))?;

    Ok(())
}
