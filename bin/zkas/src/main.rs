use std::{
    fs::{read_to_string, File},
    io::Write,
    process::exit,
};

use clap::Parser as ClapParser;

use darkfi::{
    cli_desc,
    zkas::{Analyzer, Compiler, Lexer, Parser, ZkBinary},
};

#[derive(clap::Parser)]
#[clap(name = "zkas", about = cli_desc!(), version)]
struct Args {
    /// Place the output into <FILE>
    #[clap(short = 'o', value_name = "FILE")]
    output: Option<String>,

    /// Strip debug symbols
    #[clap(short = 's')]
    strip: bool,

    /// Preprocess only; do not compile
    #[clap(short = 'E')]
    evaluate: bool,

    /// Interactive semantic analysis
    #[clap(short = 'i')]
    interactive: bool,

    /// Examine decoded bytecode
    #[clap(short = 'e')]
    examine: bool,

    /// ZK script to compile
    input: String,
}

fn main() {
    let args = Args::parse();

    let filename = args.input.as_str();
    let source = match read_to_string(filename) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed reading from \"{}\". {}", filename, e);
            exit(1);
        }
    };

    // Clean up tabs, and convert CRLF to LF.
    let source = source.replace('\t', "    ").replace("\r\n", "\n");

    // The lexer goes over the input file and separates its content into
    // tokens that get fed into a parser.
    let lexer = Lexer::new(filename, source.chars());
    let tokens = lexer.lex();

    // The parser goes over the tokens provided by the lexer and builds
    // the initial AST, not caring much about the semantics, just enforcing
    // syntax and general structure.
    let parser = Parser::new(filename, source.chars(), tokens);
    let (constants, witnesses, statements) = parser.parse();

    // The analyzer goes through the initial AST provided by the parser and
    // converts return and variable types to their correct forms, and also
    // checks that the semantics of the ZK script are correct.
    let mut analyzer = Analyzer::new(filename, source.chars(), constants, witnesses, statements);
    analyzer.analyze_types();

    if args.interactive {
        analyzer.analyze_semantic();
    }

    if args.evaluate {
        println!("{:#?}", analyzer.constants);
        println!("{:#?}", analyzer.witnesses);
        println!("{:#?}", analyzer.statements);
        println!("{:#?}", analyzer.stack);
        exit(0);
    }

    let compiler = Compiler::new(
        filename,
        source.chars(),
        analyzer.constants,
        analyzer.witnesses,
        analyzer.statements,
        analyzer.literals,
        !args.strip,
    );

    let bincode = compiler.compile();

    let output = match args.output {
        Some(o) => o,
        None => format!("{}.bin", args.input),
    };

    let mut file = match File::create(&output) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed to create \"{}\". {}", output, e);
            exit(1);
        }
    };

    if let Err(e) = file.write_all(&bincode) {
        eprintln!("Error: Failed to write bincode to \"{}\". {}", output, e);
        exit(1);
    };

    println!("Wrote output to {}", &output);

    if args.examine {
        let zkbin = ZkBinary::decode(&bincode).unwrap();
        println!("{:#?}", zkbin);
    }
}
