# Design

Conch is split into four small modules, each doing one job in the path from a raw line of text to a running pipeline of processes.

## Tokenizer (`src/lexer.rs`)

`tokenize` walks a line character by character and produces a flat `Vec<Token>`: `Word`, `Pipe`, `RedirectIn`, `RedirectOut`, `RedirectAppend`.

Quoting rules:

- Single quotes are fully literal. Nothing inside them is interpreted, not even a backslash.
- Double quotes allow escapes for `\`, `"`, and `$`, everything else passes through unchanged.
- A backslash outside quotes escapes the next character.

A trailing backslash with nothing after it, and an unterminated quote of either kind, come back as a `ShellError::Parse` instead of panicking or silently dropping input.

Safety limit: `MAX_TOKENS` (4096) caps how many tokens a single line may produce. The tokenizer checks this cap as it goes, so a hostile line built entirely of pipe characters is rejected during tokenization rather than turning into an oversized token stream first.

## Pipeline parser (`src/parser.rs`)

`parse` calls the tokenizer, then splits the resulting tokens into stages on every `Pipe` token. Each stage becomes a `Command { prog, args, redirects }`.

Within a stage, the first word is the program name, every word after it is an argument, and a redirect token consumes the word that follows it as its target. A redirect with no following word is a parse error, and an empty stage between two pipes (`a | | b`) is a parse error too.

Safety limit: `MAX_PIPELINE_DEPTH` (512) caps how many stages a pipeline may have. The check runs as pipe tokens are consumed, so a pathological `a | a | a | ...` line is rejected instead of building an unbounded `Vec<Command>`.

The output is a `Pipeline { commands: Vec<Command> }`, a plain data structure with no process handles in it. Parsing never spawns anything.

## Builtins and process wiring (`src/exec.rs`)

`run_pipeline` walks a `Pipeline` and does the actual work.

Five names are builtins, checked with a plain list: `cd`, `pwd`, `echo`, `export`, `exit`. These run inside the shell's own process because their effect only makes sense there, a child process cannot change its parent's current directory or environment.

Every other command is spawned with `std::process::Command`. For each stage:

- Its stdin is either the previous stage's stdout (taken from the child that just ran), a file opened from an input redirect, or inherited from the shell.
- Its stdout is either piped to the next stage, a file opened from an output or append redirect, or inherited from the shell.
- stderr always inherits from the shell, so errors from any stage show up immediately.

Stages are spawned in order, their `ChildStdout` handles handed to the next stage before that next stage is spawned, then every child is waited on in order. The exit code of the last stage becomes the pipeline's exit code, matching normal shell behavior.

A command that fails to spawn becomes `ShellError::UnknownCommand`, an I/O failure opening a redirect target becomes `ShellError::Io`. Neither path panics.

## Errors (`src/error.rs`)

One enum, `ShellError`, covers every failure mode: `Parse`, `UnknownCommand`, `Io`, `LimitExceeded`. Every fallible function in the crate returns `ShellResult<T> = Result<T, ShellError>`. The REPL and `-c` mode both print the error and continue or exit with a non-zero code, never unwind.

## Entry point (`src/main.rs`)

`clap` parses either a `-c "command"` flag for one-shot mode or no flag for the interactive REPL. Both paths run through the same `parser::parse` and `exec::run_pipeline` functions. The REPL caps each line at `MAX_LINE_BYTES` (1 MiB) before it is ever handed to the tokenizer, so piped input from an untrusted source cannot grow a single line without bound and exhaust memory before conch gets a chance to reject it.
