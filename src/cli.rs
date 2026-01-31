use std::path::PathBuf;

use clap::{ArgGroup, ArgMatches, arg, command, value_parser};

pub(crate) fn cli() -> ArgMatches {
    command!()
        .subcommand(
            command!("render")
                .about("Render graph to specified format")
                .subcommand(
                    command!("pdf")
                        .about("Render to PDF file")
                        .arg(arg!(-i --input <INPUT> "Raw input (inline)"))
                        .arg(
                            arg!(-f --file <INPUT> "Source file to process")
                                .value_parser(value_parser!(PathBuf)),
                        )
                        .group(
                            ArgGroup::new("input-source")
                                .args(["input", "file"])
                                .required(true)
                                .multiple(false),
                        )
                        .arg(
                            arg!(-o --output <OUTPUT> "Output to PDF File")
                                .value_parser(value_parser!(PathBuf)),
                        ),
                )
                .subcommand(
                    command!("ir")
                        .about("Render to IR file")
                        .arg(arg!(-i --input <INPUT> "Raw input (inline)"))
                        .arg(
                            arg!(-f --file <INPUT> "Source file to process")
                                .value_parser(value_parser!(PathBuf)),
                        )
                        .group(
                            ArgGroup::new("input-source")
                                .args(["input", "file"])
                                .required(true)
                                .multiple(false),
                        )
                        .arg(
                            arg!(-o --output <OUTPUT> "Output to PDF File")
                                .value_parser(value_parser!(PathBuf)),
                        ),
                ),
        )
        .subcommand(
            command!("convert")
                .about("Map a type to another")
                .arg(arg!(-i --input <INPUT> "Raw input (inline)"))
                .arg(
                    arg!(-f --file <INPUT> "Source file to process")
                        .value_parser(value_parser!(PathBuf)),
                )
                .group(
                    ArgGroup::new("input-source")
                        .args(["input", "file"])
                        .required(true)
                        .multiple(false),
                )
                .arg(
                    arg!(-o --output <OUTPUT> "Output to the converted file")
                        .value_parser(value_parser!(PathBuf)),
                ),
        )
        .get_matches()
}
