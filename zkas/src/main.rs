use anyhow::Result;
use clap::Parser as ClapParser;
use std::fs::read_to_string;

use zkas::{analyzer::Analyzer, lexer::Lexer, parser::Parser};

#[derive(clap::Parser)]
#[clap(name = "zkas", version)]
struct Cli {
    /// Place the output into <FILE>
    #[clap(short, value_name = "FILE")]
    output: Option<String>,

    /// Strip debug symbols
    #[clap(short)]
    strip: bool,

    /// Preprocess only; do not compile
    #[clap(short)]
    evaluate: bool,

    /// ZK script to compile
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let filename = cli.input.as_str();
    let source = read_to_string(filename)?;

    let lexer = Lexer::new(filename, source.chars());
    let tokens = lexer.lex();

    // println!("{:#?}", tokens);

    let parser = Parser::new(filename, source.chars(), tokens);
    let (constants, witnesses, statements) = parser.parse();

    // println!("{:#?}", constants);
    // println!("{:#?}", witnesses);
    // println!("{:#?}", statements);

    let mut analyzer = Analyzer::new(filename, source.chars(), constants, witnesses, statements);
    analyzer.analyze_types();

    Ok(())
}
