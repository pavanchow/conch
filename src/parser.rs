use crate::error::{ShellError, ShellResult};
use crate::lexer::{tokenize, Token};

/// The kind of redirect attached to one stage of a pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectKind {
    In,
    Out,
    Append,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub kind: RedirectKind,
    pub target: String,
}

/// One stage of a pipeline: a program name, its arguments, and any
/// redirects that apply to that stage.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Command {
    pub prog: String,
    pub args: Vec<String>,
    pub redirects: Vec<Redirect>,
}

/// A full command line: one or more commands connected by pipes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pipeline {
    pub commands: Vec<Command>,
}

/// Hard cap on how many stages a single pipeline may have. Guards the
/// parser against a pathological `a | a | a | ...` line that would
/// otherwise make it build an unbounded command list.
pub const MAX_PIPELINE_DEPTH: usize = 512;

/// Parse a raw command line into a [`Pipeline`]. Returns a typed
/// [`ShellError::Parse`] on malformed input; never panics.
pub fn parse(input: &str) -> ShellResult<Pipeline> {
    let tokens = tokenize(input)?;
    parse_tokens(tokens)
}

fn parse_tokens(tokens: Vec<Token>) -> ShellResult<Pipeline> {
    let mut commands = Vec::new();
    let mut stages: Vec<Vec<Token>> = vec![Vec::new()];

    for tok in tokens {
        if tok == Token::Pipe {
            stages.push(Vec::new());
            if stages.len() > MAX_PIPELINE_DEPTH {
                return Err(ShellError::LimitExceeded(format!(
                    "pipeline exceeds {MAX_PIPELINE_DEPTH} stages"
                )));
            }
        } else {
            stages.last_mut().unwrap().push(tok);
        }
    }

    if stages.iter().all(|s| s.is_empty()) {
        return Ok(Pipeline { commands });
    }

    for stage in stages {
        commands.push(parse_stage(stage)?);
    }

    for cmd in &commands {
        if cmd.prog.is_empty() {
            return Err(ShellError::Parse(
                "empty command between pipes".to_string(),
            ));
        }
    }

    Ok(Pipeline { commands })
}

fn parse_stage(tokens: Vec<Token>) -> ShellResult<Command> {
    let mut cmd = Command::default();
    let mut iter = tokens.into_iter().peekable();

    while let Some(tok) = iter.next() {
        match tok {
            Token::Word(w) => {
                if cmd.prog.is_empty() && cmd.args.is_empty() {
                    cmd.prog = w;
                } else {
                    cmd.args.push(w);
                }
            }
            Token::RedirectIn | Token::RedirectOut | Token::RedirectAppend => {
                let kind = match tok {
                    Token::RedirectIn => RedirectKind::In,
                    Token::RedirectOut => RedirectKind::Out,
                    Token::RedirectAppend => RedirectKind::Append,
                    _ => unreachable!(),
                };
                let target = match iter.next() {
                    Some(Token::Word(w)) => w,
                    _ => {
                        return Err(ShellError::Parse(
                            "redirect is missing a target".to_string(),
                        ))
                    }
                };
                cmd.redirects.push(Redirect { kind, target });
            }
            Token::Pipe => unreachable!("pipes are split before parse_stage runs"),
        }
    }

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_three_stage_pipeline() {
        let p = parse("a | b | c").unwrap();
        assert_eq!(p.commands.len(), 3);
        assert_eq!(p.commands[0].prog, "a");
        assert_eq!(p.commands[1].prog, "b");
        assert_eq!(p.commands[2].prog, "c");
    }

    #[test]
    fn parses_args_for_each_stage() {
        let p = parse("cat file.txt | grep foo | sort").unwrap();
        assert_eq!(p.commands[0].prog, "cat");
        assert_eq!(p.commands[0].args, vec!["file.txt".to_string()]);
        assert_eq!(p.commands[1].prog, "grep");
        assert_eq!(p.commands[1].args, vec!["foo".to_string()]);
        assert_eq!(p.commands[2].prog, "sort");
        assert!(p.commands[2].args.is_empty());
    }

    #[test]
    fn parses_input_and_output_redirects() {
        let p = parse("sort < in.txt > out.txt").unwrap();
        assert_eq!(p.commands.len(), 1);
        let redirects = &p.commands[0].redirects;
        assert_eq!(redirects.len(), 2);
        assert_eq!(redirects[0].kind, RedirectKind::In);
        assert_eq!(redirects[0].target, "in.txt");
        assert_eq!(redirects[1].kind, RedirectKind::Out);
        assert_eq!(redirects[1].target, "out.txt");
    }

    #[test]
    fn parses_append_redirect_on_the_last_stage() {
        let p = parse("cat file.txt | grep foo | sort > out.txt").unwrap();
        assert_eq!(p.commands.len(), 3);
        let last = &p.commands[2];
        assert_eq!(last.prog, "sort");
        assert_eq!(last.redirects.len(), 1);
        assert_eq!(last.redirects[0].kind, RedirectKind::Out);
        assert_eq!(last.redirects[0].target, "out.txt");

        let p2 = parse("printf x >> log.txt").unwrap();
        assert_eq!(p2.commands[0].redirects[0].kind, RedirectKind::Append);
    }

    #[test]
    fn empty_command_between_pipes_is_a_typed_error() {
        let err = parse("a | | b").unwrap_err();
        assert!(matches!(err, ShellError::Parse(_)));
    }

    #[test]
    fn redirect_with_no_target_is_a_typed_error() {
        let err = parse("cat >").unwrap_err();
        assert!(matches!(err, ShellError::Parse(_)));
    }

    #[test]
    fn blank_line_parses_to_an_empty_pipeline() {
        let p = parse("   ").unwrap();
        assert!(p.commands.is_empty());
    }

    #[test]
    fn malformed_input_never_panics() {
        let inputs = ["|||", "a |", "| a", "a > > b", "'''", "a \"b"];
        for i in inputs {
            let _ = parse(i);
        }
    }

    #[test]
    fn pathological_pipe_chain_is_rejected_not_looped() {
        let hostile = format!("a {}", "| a ".repeat(2000));
        let err = parse(&hostile).unwrap_err();
        assert!(matches!(err, ShellError::LimitExceeded(_)));
    }
}
