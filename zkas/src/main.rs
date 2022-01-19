use anyhow::Result;
use clap::Parser as ClapParser;
use std::{
    fs::{read_to_string, File},
    io::Write,
};

use zkas::{
    analyzer::Analyzer, compiler::Compiler, decoder::ZkBinary, lexer::Lexer, parser::Parser,
};

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

    /// Interactive semantic analysis
    #[clap(short)]
    interactive: bool,

    /// Examine decoded bytecode
    #[clap(long)]
    examine: bool,

    /// ZK script to compile
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let filename = cli.input.as_str();
    let source = read_to_string(filename)?;

    let lexer = Lexer::new(filename, source.chars());
    let tokens = lexer.lex();

    let parser = Parser::new(filename, source.chars(), tokens);
    let (constants, witnesses, statements) = parser.parse();

    let mut analyzer = Analyzer::new(filename, source.chars(), constants, witnesses, statements);
    analyzer.analyze_types();

    if cli.interactive {
        analyzer.analyze_semantic();
    }

    if cli.evaluate {
        println!("{:#?}", analyzer.constants);
        println!("{:#?}", analyzer.witnesses);
        println!("{:#?}", analyzer.statements);
        println!("{:#?}", analyzer.stack);
        return Ok(())
    }

    let compiler = Compiler::new(
        filename,
        source.chars(),
        analyzer.constants,
        analyzer.witnesses,
        analyzer.statements,
        !cli.strip,
    );

    let bincode = compiler.compile();

    let output: String;
    if let Some(o) = cli.output {
        output = o;
    } else {
        output = format!("{}.bin", cli.input);
    }

    let mut file = File::create(&output)?;
    file.write_all(&bincode)?;
    println!("Wrote output to {}", &output);

    if cli.examine {
        let zkbin = ZkBinary::decode(&bincode)?;
        println!("{:#?}", zkbin);
    }

    Ok(())
}
