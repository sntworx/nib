use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use nib_core::{Nib, Value};

#[derive(Parser)]
struct Cli {
    /// Script to run
    script: PathBuf,

    /// Print the parsed AST instead of running the script, optionally saving it to FILE
    #[arg(long, num_args = 0..=1, default_missing_value = "-", value_name = "FILE")]
    ast: Option<PathBuf>,

    /// Library file(s) to include before the script, comma-separated
    #[arg(long, value_delimiter = ',', value_name = "FILE")]
    include: Vec<PathBuf>,

    /// Measure and print script execution time
    #[arg(long)]
    time: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let source = match fs::read_to_string(&cli.script) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to read script '{}': {}", cli.script.display(), e);
            return ExitCode::FAILURE;
        }
    };

    let mut nib = Nib::new();

    nib.register_func("print", |args: &[Value]| {
        let rendered = args
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        println!("{}", rendered);
        Ok(Value::Null)
    });

    for path in &cli.include {
        let include_source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("failed to read include '{}': {}", path.display(), e);
                return ExitCode::FAILURE;
            }
        };
        nib.include(include_source);
    }

    if let Err(e) = nib.parse(&source) {
        eprintln!("{}", e);
        return ExitCode::FAILURE;
    }

    match cli.ast {
        None => {
            let start = Instant::now();
            let result = nib.run();
            let elapsed = start.elapsed();

            if let Err(e) = result {
                eprintln!("{}", e);
                return ExitCode::FAILURE;
            }

            if cli.time {
                println!("Execution time: {:.3?}", elapsed);
            }
        }
        Some(path) if path == PathBuf::from("-") => {
            let ast = nib
                .ast()
                .expect("ast is always set after a successful parse");
            println!("{:#?}", ast);
        }
        Some(path) => {
            let ast = nib
                .ast()
                .expect("ast is always set after a successful parse");
            let contents = format!("{:#?}", ast);
            if let Err(e) = fs::write(&path, contents) {
                eprintln!("Failed to write AST to '{}': {}", path.display(), e);
                return ExitCode::FAILURE;
            }
            println!("AST written to '{}'", path.display());
        }
    }

    ExitCode::SUCCESS
}
