use anyhow::Result;
use clap::clap_app;
use std::fs::read_to_string;

use zkas::{lexer::lex, parser::parse};

fn main() -> Result<()> {
    let args = clap_app!(zkas =>
        (@arg INPUT: +required "ZK script to compile")
    )
    .get_matches();

    let filename = args.value_of("INPUT").unwrap();
    let source = read_to_string(filename)?;
    let tokens = lex(filename, source.chars());

    println!("{:#?}", tokens);

    let ast = parse(filename, source.chars(), tokens);

    Ok(())
}
