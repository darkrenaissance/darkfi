use anyhow::Result;
use clap::clap_app;
use std::fs::read_to_string;

use zkas::{analyzer::Analyzer, lexer::Lexer, parser::Parser};

fn main() -> Result<()> {
    let args = clap_app!(zkas =>
        (@arg strip: -s "Strip debug symbols")
        (@arg preprocess: -E "Preprocess only; do not compile")
        (@arg OUTPUT: -o +takes_value "Place the output into <OUTPUT>")
        (@arg INPUT: +required "ZK script to compile")
    )
    .get_matches();

    let filename = args.value_of("INPUT").unwrap();
    let source = read_to_string(filename)?;

    let lexer = Lexer::new(filename, source.chars());
    let tokens = lexer.lex();

    // println!("{:#?}", tokens);

    let parser = Parser::new(filename, source.chars(), tokens);
    let (constants, witnesses, statements) = parser.parse();

    // println!("{:#?}", constants);
    // println!("{:#?}", witnesses);
    // println!("{:#?}", statements);

    let analyzer = Analyzer::new(filename, source.chars(), constants, witnesses, statements);
    analyzer.analyze();

    Ok(())
}
