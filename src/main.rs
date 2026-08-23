use std::io::{self, BufRead, Read, Write};

use clap::Parser as ClapParser;
use conch::{exec, parser};

/// Hard cap on how many bytes a single input line may hold. Piped input
/// from an untrusted source cannot grow a line past this and exhaust
/// memory before it ever reaches the tokenizer.
const MAX_LINE_BYTES: usize = 1 << 20; // 1 MiB

#[derive(ClapParser, Debug)]
#[command(name = "conch", version, about = "A shell in Rust, readable end to end.")]
struct Args {
    /// Run a single command line and exit, instead of starting the REPL.
    #[arg(short = 'c', value_name = "COMMAND")]
    command: Option<String>,
}

fn main() {
    let args = Args::parse();

    let code = match args.command {
        Some(line) => run_line(&line),
        None => repl(),
    };

    std::process::exit(code);
}

fn repl() -> i32 {
    let stdin = io::stdin();
    let mut last_code = 0;

    loop {
        print!("conch> ");
        if io::stdout().flush().is_err() {
            break;
        }

        let mut line = String::new();
        let mut handle = stdin.lock().take(MAX_LINE_BYTES as u64 + 1);
        let n = match handle.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(e) => {
                eprintln!("conch: read error: {e}");
                last_code = 1;
                continue;
            }
        };

        if n as usize > MAX_LINE_BYTES {
            eprintln!("conch: line too long, rejected (limit {MAX_LINE_BYTES} bytes)");
            last_code = 1;
            continue;
        }

        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.trim().is_empty() {
            continue;
        }
        if trimmed.trim() == "exit" {
            break;
        }

        last_code = run_line(trimmed);
    }

    last_code
}

fn run_line(line: &str) -> i32 {
    if line.len() > MAX_LINE_BYTES {
        eprintln!("conch: line too long, rejected (limit {MAX_LINE_BYTES} bytes)");
        return 1;
    }

    match parser::parse(line) {
        Ok(pipeline) => {
            if pipeline.commands.is_empty() {
                return 0;
            }
            match exec::run_pipeline(&pipeline) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}
