use anyhow::Result;
use clap::clap_app;
use std::fs::File;
use std::io::{BufRead, BufReader};

use zkas::compiler::{CompiledContract, Compiler};
use zkas::output::{bincode_output, text_output};
use zkas::parser::{load_lines, parse_lines};

fn main() -> Result<()> {
    let args = clap_app!(zkas =>
        (@arg INPUT: +required "ZK script to compile")
        (@arg DISPLAY: -d --display "Show the compiled code in human readable format")
        (@arg OUTPUT: -o --output +takes_value "Output file")
    )
    .get_matches();

    let file = File::open(args.value_of("INPUT").unwrap())?;
    let lines = load_lines(BufReader::new(file).lines());
    //println!("{:#?}", lines);
    let syntax = parse_lines(lines);
    //println!("{:#?}", syntax);
    let schema = syntax.verify();
    //println!("{:#?}", schema);

    let mut contracts = vec![];
    for (name, witness, uncompiled_code) in schema {
        let compiler = Compiler::new(witness.clone(), uncompiled_code, syntax.constants.clone());
        let code = compiler.compile();
        //println!("{:#?}", code);
        contracts.push(CompiledContract::new(name, witness, code));
    }

    if args.is_present("DISPLAY") {
        text_output(contracts.clone(), syntax.constants.clone())?;
    }

    let output_file = if args.is_present("OUTPUT") {
        match args.value_of("OUTPUT").unwrap() {
            "-" => {
                println!("Unable to output compiled code to stdout");
                std::process::exit(1);
            }
            v => v.to_string(),
        }
    } else {
        format!("{}.bin", args.value_of("INPUT").unwrap())
    };

    bincode_output(&output_file, contracts, syntax.constants)?;

    Ok(())
}
