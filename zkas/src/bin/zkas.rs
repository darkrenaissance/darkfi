use anyhow::Result;
use clap::clap_app;
use std::fs::read_to_string;

use zkas::{lexer::Lexer, parser::Parser};

fn main() -> Result<()> {
    let args = clap_app!(zkas =>
        (@arg INPUT: +required "ZK script to compile")
    )
    .get_matches();

    let filename = args.value_of("INPUT").unwrap();
    let source = read_to_string(filename)?;

    let lexer = Lexer::new(filename, source.chars());
    let tokens = lexer.lex();

    // println!("{:#?}", tokens);

    let parser = Parser::new(filename, source.chars(), tokens);
    let (constants, witnesses, circuit) = parser.parse();

    // println!("{:#?}", constants);
    // println!("{:#?}", witnesses);
    // println!("{:#?}", circuit);

    Ok(())
}
