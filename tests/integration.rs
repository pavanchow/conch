// Integration tests driving conch through its public API, the way a
// downstream consumer of the crate would.

use conch::exec::{is_builtin, run_pipeline};
use conch::lexer::{tokenize, Token, MAX_TOKENS};
use conch::parser::{RedirectKind, MAX_PIPELINE_DEPTH};
use conch::{parse, Pipeline, ShellError};

#[test]
fn round_trip_full_pipeline_structure() {
    let p = parse("cat file.txt | grep foo | sort > out.txt").unwrap();
    assert_eq!(p.commands.len(), 3);

    assert_eq!(p.commands[0].prog, "cat");
    assert_eq!(p.commands[0].args, vec!["file.txt".to_string()]);
    assert!(p.commands[0].redirects.is_empty());

    assert_eq!(p.commands[1].prog, "grep");
    assert_eq!(p.commands[1].args, vec!["foo".to_string()]);

    let last = &p.commands[2];
    assert_eq!(last.prog, "sort");
    assert_eq!(last.redirects.len(), 1);
    assert_eq!(last.redirects[0].kind, RedirectKind::Out);
    assert_eq!(last.redirects[0].target, "out.txt");
}

#[test]
fn empty_and_blank_input_parse_to_empty_pipeline() {
    assert!(parse("").unwrap().commands.is_empty());
    assert!(parse("      \t  ").unwrap().commands.is_empty());
    // An empty pipeline is a no-op that runs cleanly.
    assert_eq!(run_pipeline(&Pipeline::default()).unwrap(), 0);
}

#[test]
fn minimum_single_word_command() {
    let p = parse("ls").unwrap();
    assert_eq!(p.commands.len(), 1);
    assert_eq!(p.commands[0].prog, "ls");
    assert!(p.commands[0].args.is_empty());
    assert!(p.commands[0].redirects.is_empty());
}

#[test]
fn quoting_semantics_through_public_tokenizer() {
    // Single quotes are literal (no escape processing).
    let toks = tokenize("echo 'a b  c'").unwrap();
    assert_eq!(
        toks,
        vec![Token::Word("echo".into()), Token::Word("a b  c".into())]
    );

    // Double quotes recognize \\, \" and \$ escapes.
    let toks = tokenize(r#"echo "say \"hi\" $x""#).unwrap();
    assert_eq!(
        toks,
        vec![
            Token::Word("echo".into()),
            Token::Word("say \"hi\" $x".into()),
        ]
    );

    // Backslash escaping outside quotes joins a split word.
    let toks = tokenize(r"foo\ bar").unwrap();
    assert_eq!(toks, vec![Token::Word("foo bar".into())]);
}

#[test]
fn all_redirect_kinds_parse() {
    let p = parse("sort < in.txt >> log.txt").unwrap();
    let r = &p.commands[0].redirects;
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].kind, RedirectKind::In);
    assert_eq!(r[0].target, "in.txt");
    assert_eq!(r[1].kind, RedirectKind::Append);
    assert_eq!(r[1].target, "log.txt");
}

#[test]
fn pipeline_at_the_depth_limit_is_accepted() {
    // MAX_PIPELINE_DEPTH stages (MAX_PIPELINE_DEPTH - 1 pipes) must parse:
    // guards the off-by-one at the boundary.
    let line = vec!["a"; MAX_PIPELINE_DEPTH].join(" | ");
    let p = parse(&line).unwrap();
    assert_eq!(p.commands.len(), MAX_PIPELINE_DEPTH);
}

#[test]
fn pipeline_one_past_the_limit_is_rejected() {
    let line = vec!["a"; MAX_PIPELINE_DEPTH + 1].join(" | ");
    let err = parse(&line).unwrap_err();
    assert!(matches!(err, ShellError::LimitExceeded(_)));
}

#[test]
fn token_flood_is_rejected_not_looped() {
    let hostile = "|".repeat(MAX_TOKENS * 4);
    let err = parse(&hostile).unwrap_err();
    assert!(matches!(err, ShellError::LimitExceeded(_)));
    // Same via the tokenizer directly.
    let err = tokenize(&hostile).unwrap_err();
    assert!(matches!(err, ShellError::LimitExceeded(_)));
}

#[test]
fn every_parse_error_path_is_typed() {
    // Unterminated quotes.
    assert!(matches!(parse("echo 'oops"), Err(ShellError::Parse(_))));
    assert!(matches!(parse("echo \"oops"), Err(ShellError::Parse(_))));
    // Trailing backslash.
    assert!(matches!(parse("echo foo\\"), Err(ShellError::Parse(_))));
    // Redirect with no target.
    assert!(matches!(parse("cat >"), Err(ShellError::Parse(_))));
    // Empty command between pipes.
    assert!(matches!(parse("a | | b"), Err(ShellError::Parse(_))));
}

#[test]
fn parse_is_deterministic() {
    let line = "cat a.txt | grep x > b.txt";
    assert_eq!(parse(line).unwrap(), parse(line).unwrap());
}

#[test]
fn builtin_classification_is_exposed() {
    for b in ["cd", "pwd", "echo", "export", "exit"] {
        assert!(is_builtin(b), "{b} should be a builtin");
    }
    assert!(!is_builtin("cat"));
    assert!(!is_builtin("definitely_not_a_builtin"));
}

#[test]
fn run_pipeline_external_exit_codes() {
    // `true` and `false` are on every POSIX box; check the last-stage code
    // is propagated exactly.
    assert_eq!(run_pipeline(&parse("true").unwrap()).unwrap(), 0);
    assert_eq!(run_pipeline(&parse("false").unwrap()).unwrap(), 1);
}

#[test]
fn run_pipeline_unknown_command_is_typed_error() {
    let p = parse("this_binary_does_not_exist_9f3a2b").unwrap();
    let err = run_pipeline(&p).unwrap_err();
    assert!(matches!(err, ShellError::UnknownCommand(_)));
}
