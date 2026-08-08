use std::fs;
use std::io::{self, Write};
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

    nib.register_func("println", |args: &[Value]| {
        println!("{}", render_print(args));
        Ok(Value::Null)
    });

    nib.register_func("print", |args: &[Value]| {
        print!("{}", render_print(args));
        // print! has no trailing newline, so a line-buffered stdout won't
        // flush it on its own - matters when a prompt is meant to appear
        // right before a blocking read().
        io::stdout()
            .flush()
            .map_err(|e| format!("failed to flush stdout: {}", e))?;
        Ok(Value::Null)
    });

    nib.register_func("read", |args: &[Value]| {
        if !args.is_empty() {
            return Err(format!("'read' expects 0 arguments, got {}", args.len()));
        }
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("failed to read input: {}", e))?;
        let trimmed = input.trim_end_matches(['\n', '\r']);
        Ok(Value::Str(trimmed.to_string()))
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

fn render_print(args: &[Value]) -> String {
    args.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
