use crate::error::{ShellError, ShellResult};

/// A single lexical token out of a raw command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A word: a command name, an argument, or a redirect target. Quotes
    /// and escapes have already been stripped by the time this appears.
    Word(String),
    /// `|`
    Pipe,
    /// `<`
    RedirectIn,
    /// `>`
    RedirectOut,
    /// `>>`
    RedirectAppend,
}

/// Hard cap on how many tokens a single line may produce. A line that would
/// exceed this is rejected outright rather than tokenized, so a hostile or
/// malformed input cannot make the parser walk an unbounded token stream.
pub const MAX_TOKENS: usize = 4096;

/// Turn a raw line into a stream of tokens, honoring single quotes (literal,
/// no escapes), double quotes (escapes for `\`, `"`, `$` recognized), and
/// backslash escapes outside quotes. Never panics: malformed quoting comes
/// back as a `ShellError::Parse`.
pub fn tokenize(input: &str) -> ShellResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;
    let len = chars.len();

    let mut word = String::new();
    let mut in_word = false;

    macro_rules! flush_word {
        () => {
            if in_word {
                tokens.push(Token::Word(std::mem::take(&mut word)));
                in_word = false;
            }
        };
    }

    while i < len {
        if tokens.len() > MAX_TOKENS {
            return Err(ShellError::LimitExceeded(format!(
                "line produced more than {MAX_TOKENS} tokens"
            )));
        }

        let c = chars[i];

        match c {
            ' ' | '\t' => {
                flush_word!();
                i += 1;
            }
            '\'' => {
                in_word = true;
                i += 1;
                let start = i;
                loop {
                    if i >= len {
                        return Err(ShellError::Parse(format!(
                            "unterminated single quote starting at column {start}"
                        )));
                    }
                    if chars[i] == '\'' {
                        i += 1;
                        break;
                    }
                    word.push(chars[i]);
                    i += 1;
                }
            }
            '"' => {
                in_word = true;
                i += 1;
                loop {
                    if i >= len {
                        return Err(ShellError::Parse(
                            "unterminated double quote".to_string(),
                        ));
                    }
                    match chars[i] {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' if i + 1 < len
                            && matches!(chars[i + 1], '"' | '\\' | '$') =>
                        {
                            word.push(chars[i + 1]);
                            i += 2;
                        }
                        other => {
                            word.push(other);
                            i += 1;
                        }
                    }
                }
            }
            '\\' => {
                if i + 1 >= len {
                    return Err(ShellError::Parse(
                        "trailing backslash with nothing to escape".to_string(),
                    ));
                }
                in_word = true;
                word.push(chars[i + 1]);
                i += 2;
            }
            '|' => {
                flush_word!();
                tokens.push(Token::Pipe);
                i += 1;
            }
            '>' => {
                flush_word!();
                if i + 1 < len && chars[i + 1] == '>' {
                    tokens.push(Token::RedirectAppend);
                    i += 2;
                } else {
                    tokens.push(Token::RedirectOut);
                    i += 1;
                }
            }
            '<' => {
                flush_word!();
                tokens.push(Token::RedirectIn);
                i += 1;
            }
            other => {
                in_word = true;
                word.push(other);
                i += 1;
            }
        }
    }

    flush_word!();

    if tokens.len() > MAX_TOKENS {
        return Err(ShellError::LimitExceeded(format!(
            "line produced more than {MAX_TOKENS} tokens"
        )));
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_plain_words() {
        let toks = tokenize("echo hello world").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Word("echo".into()),
                Token::Word("hello".into()),
                Token::Word("world".into()),
            ]
        );
    }

    #[test]
    fn single_quotes_are_literal() {
        let toks = tokenize("echo 'a b  c' '\\n'").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Word("echo".into()),
                Token::Word("a b  c".into()),
                Token::Word("\\n".into()),
            ]
        );
    }

    #[test]
    fn double_quotes_allow_escapes() {
        let toks = tokenize(r#"echo "say \"hi\" to $USER""#).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Word("echo".into()),
                Token::Word("say \"hi\" to $USER".into()),
            ]
        );
    }

    #[test]
    fn backslash_escapes_outside_quotes() {
        let toks = tokenize(r"echo foo\ bar").unwrap();
        assert_eq!(
            toks,
            vec![Token::Word("echo".into()), Token::Word("foo bar".into())]
        );
    }

    #[test]
    fn pipes_and_redirects_are_tokenized() {
        let toks = tokenize("a < in.txt | b > out.txt | c >> log.txt").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Word("a".into()),
                Token::RedirectIn,
                Token::Word("in.txt".into()),
                Token::Pipe,
                Token::Word("b".into()),
                Token::RedirectOut,
                Token::Word("out.txt".into()),
                Token::Pipe,
                Token::Word("c".into()),
                Token::RedirectAppend,
                Token::Word("log.txt".into()),
            ]
        );
    }

    #[test]
    fn unterminated_single_quote_is_a_typed_error() {
        let err = tokenize("echo 'unterminated").unwrap_err();
        assert!(matches!(err, ShellError::Parse(_)));
    }

    #[test]
    fn unterminated_double_quote_is_a_typed_error() {
        let err = tokenize("echo \"unterminated").unwrap_err();
        assert!(matches!(err, ShellError::Parse(_)));
    }

    #[test]
    fn trailing_backslash_is_a_typed_error() {
        let err = tokenize("echo foo\\").unwrap_err();
        assert!(matches!(err, ShellError::Parse(_)));
    }

    #[test]
    fn does_not_panic_on_pathological_input() {
        let hostile = "|".repeat(50_000);
        let result = tokenize(&hostile);
        assert!(result.is_err());
    }
}
