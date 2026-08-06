use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use lame_core::{Lame, Value};

#[derive(Parser)]
struct Cli {
    /// Script to run
    script: PathBuf,

    /// Print the parsed AST instead of running the script, optionally saving it to FILE
    #[arg(long, num_args = 0..=1, default_missing_value = "-", value_name = "FILE")]
    ast: Option<PathBuf>,
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

    let mut lame = Lame::new();

    lame.register_func("print", |args: &[Value]| {
        let rendered = args
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        println!("{}", rendered);
        Ok(Value::Null)
    });

    if let Err(e) = lame.parse(&source) {
        eprintln!("{}", e);
        return ExitCode::FAILURE;
    }

    match cli.ast {
        None => {
            if let Err(e) = lame.run() {
                eprintln!("{}", e);
                return ExitCode::FAILURE;
            }
        }
        Some(path) if path == PathBuf::from("-") => {
            let ast = lame
                .ast()
                .expect("ast is always set after a successful parse");
            println!("{:#?}", ast);
        }
        Some(path) => {
            let ast = lame
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
