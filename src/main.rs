mod parser;
use parser::grammar::parse;

use std::fs::read_to_string;
use std::io::Result;

fn main() -> Result<()> {
    let file_content = read_to_string("src/test.graph")?;
    let graph = parse(file_content);
    println!("{graph:#?}");
    Ok(())
}
