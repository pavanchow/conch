use std::fmt;

/// Every failure mode conch can hit while reading, parsing, or running a
/// command line. Nothing in this crate panics on bad input; it returns one
/// of these instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellError {
    /// The line could not be tokenized or parsed into a pipeline.
    Parse(String),
    /// A builtin or external program name that resolves to nothing runnable.
    UnknownCommand(String),
    /// A redirect target or working directory could not be opened.
    Io(String),
    /// The input exceeded a hard safety limit (line length, token count,
    /// or pipeline depth) and was rejected instead of processed.
    LimitExceeded(String),
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShellError::Parse(msg) => write!(f, "conch: parse error: {msg}"),
            ShellError::UnknownCommand(cmd) => write!(f, "conch: {cmd}: command not found"),
            ShellError::Io(msg) => write!(f, "conch: {msg}"),
            ShellError::LimitExceeded(msg) => write!(f, "conch: input rejected: {msg}"),
        }
    }
}

impl std::error::Error for ShellError {}

pub type ShellResult<T> = Result<T, ShellError>;
