mod cli;
use cli::cli;

use std::path::PathBuf;

use concurrent::{Error, convert_graph, process_graph_to_ir, process_graph_to_pdf};

type AppResult<T> = Result<T, Error>;

const DEFAULT_OUTPUT: &str = "render/output.pdf";

fn main() -> AppResult<()> {
    let cmd = cli();

    match cmd.subcommand() {
        Some(("render", render_cmd)) => match render_cmd.subcommand() {
            Some(("pdf", args)) => render_pdf(args),
            Some(("ir", args)) => render_ir(args),
            _ => Err(Error::InvalidParams),
        },
        Some(("convert", cmd)) => convert(cmd),
        _ => Err(Error::InvalidParams),
    }?;

    Ok(())
}

fn render_pdf(args: &clap::ArgMatches) -> AppResult<()> {
    let (input, ext) = if let Some(inline) = args.get_one::<String>("input") {
        (inline.clone(), "graph")
    } else if let Some(file_path) = args.get_one::<PathBuf>("file") {
        let ext = file_path
            .extension()
            .expect("File must have an extension")
            .to_str()
            .unwrap();
        (
            std::fs::read_to_string(file_path)
                .map_err(|e| Error::ParseError(format!("Failed to read file: {e}")))?,
            ext,
        )
    } else {
        return Err(Error::InvalidParams);
    };

    let output_path = args
        .get_one::<PathBuf>("output")
        .cloned()
        .unwrap_or(PathBuf::from(DEFAULT_OUTPUT));

    process_graph_to_pdf(&input, &output_path, ext)
}

fn render_ir(args: &clap::ArgMatches) -> AppResult<()> {
    let (input, ext) = if let Some(inline) = args.get_one::<String>("input") {
        (inline.clone(), "graph")
    } else if let Some(file_path) = args.get_one::<PathBuf>("file") {
        let ext = file_path
            .extension()
            .expect("File must have an extension")
            .to_str()
            .unwrap();
        (
            std::fs::read_to_string(file_path)
                .map_err(|e| Error::ParseError(format!("Failed to read file: {e}")))?,
            ext,
        )
    } else {
        return Err(Error::InvalidParams);
    };

    let output_path = args
        .get_one::<PathBuf>("output")
        .cloned()
        .unwrap_or(PathBuf::from(DEFAULT_OUTPUT));

    process_graph_to_ir(&input, &output_path, ext)
}

fn convert(args: &clap::ArgMatches) -> AppResult<()> {
    let (input, ext) = if let Some(inline) = args.get_one::<String>("input") {
        (inline.clone(), "graph")
    } else if let Some(file_path) = args.get_one::<PathBuf>("file") {
        let ext = file_path
            .extension()
            .expect("File must have an extension")
            .to_str()
            .unwrap();
        (
            std::fs::read_to_string(file_path)
                .map_err(|e| Error::ParseError(format!("Failed to read file: {e}")))?,
            ext,
        )
    } else {
        return Err(Error::InvalidParams);
    };

    let output_ext = args
        .get_one::<PathBuf>("output")
        .expect("Output extension is required");

    convert_graph(&input, output_ext, ext)?;

    Ok(())
}
