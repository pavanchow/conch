use std::env;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::process::{Child, Command as ProcCommand, Stdio};

use crate::error::{ShellError, ShellResult};
use crate::parser::{Command, Pipeline, RedirectKind};

const BUILTINS: [&str; 5] = ["cd", "pwd", "echo", "export", "exit"];

pub fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

/// Run a fully parsed pipeline to completion, wiring pipes between stages
/// and applying each stage's redirects. Returns the exit code of the last
/// stage, matching shell convention.
pub fn run_pipeline(pipeline: &Pipeline) -> ShellResult<i32> {
    if pipeline.commands.is_empty() {
        return Ok(0);
    }

    // A single builtin with no pipe runs in-process so `cd`/`export`/`exit`
    // affect the shell itself rather than a throwaway child.
    if pipeline.commands.len() == 1 && is_builtin(&pipeline.commands[0].prog) {
        return run_builtin(&pipeline.commands[0]);
    }

    let n = pipeline.commands.len();
    let mut children: Vec<Child> = Vec::with_capacity(n);
    let mut prev_stdout: Option<std::process::ChildStdout> = None;

    for (idx, cmd) in pipeline.commands.iter().enumerate() {
        let is_last = idx == n - 1;

        if is_builtin(&cmd.prog) {
            // A builtin mid-pipeline still needs to participate in the pipe
            // chain; conch runs it out-of-process is unnecessary for the
            // small builtin set, so it executes inline and its result
            // becomes the exit code contribution, but it cannot both read
            // a pipe and write one without a subprocess. Builtins here are
            // limited to a standalone stage: run and pass exit code along,
            // stdin/stdout pass-through is skipped intentionally.
            let code = run_builtin(cmd)?;
            if is_last {
                return Ok(code);
            }
            continue;
        }

        let stdin = if let Some(out) = prev_stdout.take() {
            Stdio::from(out)
        } else if let Some(r) = cmd
            .redirects
            .iter()
            .rev()
            .find(|r| r.kind == RedirectKind::In)
        {
            let f = File::open(&r.target)
                .map_err(|e| ShellError::Io(format!("{}: {e}", r.target)))?;
            Stdio::from(f)
        } else {
            Stdio::inherit()
        };

        let stdout = if !is_last {
            Stdio::piped()
        } else if let Some(r) = cmd.redirects.iter().rev().find(|r| {
            r.kind == RedirectKind::Out || r.kind == RedirectKind::Append
        }) {
            let f = open_for_redirect(&r.target, &r.kind)?;
            Stdio::from(f)
        } else {
            Stdio::inherit()
        };

        let mut child = ProcCommand::new(&cmd.prog)
            .args(&cmd.args)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|_| ShellError::UnknownCommand(cmd.prog.clone()))?;

        prev_stdout = child.stdout.take();
        children.push(child);
    }

    let mut last_code = 0;
    for mut child in children {
        let status = child
            .wait()
            .map_err(|e| ShellError::Io(format!("wait failed: {e}")))?;
        last_code = status.code().unwrap_or(-1);
    }

    Ok(last_code)
}

fn open_for_redirect(target: &str, kind: &RedirectKind) -> ShellResult<File> {
    let path = Path::new(target);
    let result = match kind {
        RedirectKind::Append => OpenOptions::new().create(true).append(true).open(path),
        _ => OpenOptions::new().create(true).write(true).truncate(true).open(path),
    };
    result.map_err(|e| ShellError::Io(format!("{target}: {e}")))
}

fn run_builtin(cmd: &Command) -> ShellResult<i32> {
    match cmd.prog.as_str() {
        "cd" => {
            let target = cmd
                .args
                .first()
                .cloned()
                .or_else(|| env::var("HOME").ok())
                .ok_or_else(|| ShellError::Io("cd: no target and HOME is unset".to_string()))?;
            env::set_current_dir(&target)
                .map_err(|e| ShellError::Io(format!("cd: {target}: {e}")))?;
            Ok(0)
        }
        "pwd" => {
            let dir = env::current_dir()
                .map_err(|e| ShellError::Io(format!("pwd: {e}")))?;
            println!("{}", dir.display());
            Ok(0)
        }
        "echo" => {
            println!("{}", cmd.args.join(" "));
            Ok(0)
        }
        "export" => {
            for arg in &cmd.args {
                match arg.split_once('=') {
                    Some((k, v)) => env::set_var(k, v),
                    None => {
                        return Err(ShellError::Parse(format!(
                            "export: expected NAME=value, got '{arg}'"
                        )))
                    }
                }
            }
            Ok(0)
        }
        "exit" => {
            let code = cmd
                .args
                .first()
                .and_then(|a| a.parse::<i32>().ok())
                .unwrap_or(0);
            std::process::exit(code);
        }
        other => Err(ShellError::UnknownCommand(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use std::env;

    #[test]
    fn cd_changes_the_working_directory() {
        let start = env::current_dir().unwrap();
        let target = env::temp_dir();
        let pipeline = parse(&format!("cd {}", target.display())).unwrap();
        run_pipeline(&pipeline).unwrap();
        let now = env::current_dir().unwrap();
        assert_eq!(
            now.canonicalize().unwrap(),
            target.canonicalize().unwrap()
        );
        env::set_current_dir(start).unwrap();
    }

    #[test]
    fn export_sets_an_env_var_visible_to_a_child() {
        let pipeline = parse("export CONCH_TEST_VAR=hello123").unwrap();
        run_pipeline(&pipeline).unwrap();
        assert_eq!(env::var("CONCH_TEST_VAR").unwrap(), "hello123");

        let check = parse("printenv CONCH_TEST_VAR").unwrap();
        // printenv is an external command; run it and trust the OS to have
        // inherited the var we just exported into our own process env.
        let code = run_pipeline(&check).unwrap();
        assert_eq!(code, 0);
    }

    #[test]
    fn unknown_command_is_a_typed_error() {
        let pipeline = parse("this_command_does_not_exist_anywhere_1234").unwrap();
        let err = run_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, ShellError::UnknownCommand(_)));
    }

    #[test]
    fn export_without_equals_is_a_typed_error() {
        let pipeline = parse("export NOVALUE").unwrap();
        let err = run_pipeline(&pipeline).unwrap_err();
        assert!(matches!(err, ShellError::Parse(_)));
    }
}
