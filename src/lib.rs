pub mod error;
pub mod exec;
pub mod lexer;
pub mod parser;

pub use error::{ShellError, ShellResult};
pub use parser::{parse, Pipeline};
