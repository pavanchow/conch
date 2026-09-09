# Conch: a Unix shell in Rust

<img src="docs/logo.svg" alt="Conch logo" width="96">

Conch is a Unix shell written from scratch in Rust, small enough to read end to end. It has a hand-written tokenizer, a pipeline parser that understands pipes and redirects, and process wiring that spawns real commands and connects them the way a shell should, with no shell crate underneath and only `clap` for argument parsing. Use it as a readable reference implementation of how a shell tokenizes, parses, and runs a command pipeline.

**[Live demo](https://pavanchow.github.io/conch/)** · MIT licensed · written in Rust

## What it is

- A tokenizer that handles words, single and double quotes, and backslash escapes, guarded by a token cap so hostile input cannot make it loop or grow without bound.
- A parser that turns a token stream into a pipeline of commands, each with its own arguments and redirects (`<`, `>`, `>>`), guarded by a pipeline depth cap.
- Five builtins that run inside the shell's own process: `cd`, `pwd`, `echo`, `export`, `exit`.
- Every other command spawned with `std::process::Command`, piped stage to stage, with file redirects opened where the pipeline says to open them.
- Typed errors everywhere a shell can fail: a parse error, an unknown command, an I/O failure, an input that hit a safety limit. Nothing panics on bad input.

## Usage

Build it:

```
cargo build --release
```

Run one command and exit:

```
conch -c "cat file.txt | grep foo | sort > out.txt"
```

Or start the interactive REPL:

```
conch
conch> echo hello | tr a-z A-Z
HELLO
conch> exit
```

## Tests

```
cargo test
```

Covers the tokenizer's quoting and escaping, the parser building the right pipeline for `a | b | c` and for redirects, `cd` actually changing the working directory, `export` setting a variable a child process can see, an unknown command returning a typed error, and malformed input never panicking.

See `DESIGN.md` for how the tokenizer, parser, and process wiring fit together.

By Pavan Nallamothu.
