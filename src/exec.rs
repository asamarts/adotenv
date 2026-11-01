//! A command callback that runs `$(command)` forms through the system shell.
//!
//! This module exists only with the `exec` feature. [`Shell`] is a ready-made
//! callback for
//! [`Document::resolve_with_commands`](crate::Document::resolve_with_commands):
//! it runs each command with `sh -c`, or `cmd /C` on Windows, in a directory
//! you name, and returns its standard output.
//!
//! A command in a `.env` file is arbitrary code that runs with the permissions
//! of your process. Use [`Shell`] only with files you trust like your own code,
//! and review the commands they contain. Nothing else in this crate starts a
//! process: without this module, commands run only through a callback you
//! write.

use std::{collections::BTreeMap, path::PathBuf, process::Command};

use crate::{CommandContext, CommandError};

#[cfg(windows)]
const SHELL: &str = "cmd";
#[cfg(windows)]
const SHELL_FLAG: &str = "/C";
#[cfg(not(windows))]
const SHELL: &str = "sh";
#[cfg(not(windows))]
const SHELL_FLAG: &str = "-c";

/// A command runner that executes `$(command)` forms through the system shell.
///
/// [`Shell::callback`] turns it into the callback that
/// [`Document::resolve_with_commands`](crate::Document::resolve_with_commands)
/// takes. Each command:
///
/// - runs as `sh -c <command>`, or as `cmd /C <command>` on Windows, with the
///   command text exactly as written between `$(` and `)`, so it uses that
///   shell's syntax;
/// - runs in the directory given to [`Shell::new`], or in the directory that
///   [`Shell::with_origins`] maps its source identity to;
/// - inherits the environment of the current process. The shell expands
///   variable references in the command from that environment, not from the
///   file or the lookup closure;
/// - gets an empty standard input. Its standard error is captured and appears
///   only in the error message of a failed command.
///
/// The result is the command's standard output with one trailing newline
/// removed (`\n` or `\r\n`). A command that cannot start, exits with a nonzero
/// status or prints output that is not UTF-8 fails with a [`CommandError`],
/// and resolution stops with
/// [`ResolveError::Command`](crate::ResolveError::Command).
///
/// Commands run with the permissions of your process. Use `Shell` only with
/// trusted files, and wrap [`Shell::run`] in your own callback when you want to
/// allow only some commands.
///
/// # Examples
///
/// ```no_run
/// use adotenv::{exec::Shell, parse_strict, ExpandOptions};
///
/// let doc = parse_strict("REVISION=$(git rev-parse --short HEAD)\n")?;
/// let shell = Shell::new("/srv/app");
/// let resolved = doc.resolve_with_commands(
///     |_| None,
///     shell.callback(),
///     None,
///     &ExpandOptions::dotenv_expand(),
/// )?;
/// assert_eq!(resolved.commands().len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone)]
pub struct Shell {
    directory: PathBuf,
    origins: BTreeMap<String, PathBuf>,
}

impl Shell {
    /// Creates a runner that uses `directory` for every command.
    ///
    /// A directory that does not exist is reported as an error when a command
    /// runs; the current directory of the process is never used instead. A
    /// relative path is taken relative to the current directory at the time
    /// each command starts.
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            origins: BTreeMap::new(),
        }
    }

    /// Creates a runner that picks the working directory by source identity.
    ///
    /// A command whose [`CommandContext::source`] is a key of `origins` runs in
    /// the mapped directory; any other command, including one without a source
    /// identity, runs in `directory`. Keys must match the `source_identity`
    /// passed to
    /// [`Document::resolve_with_commands`](crate::Document::resolve_with_commands)
    /// exactly. This lets commands from each file run next to that file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{collections::BTreeMap, path::PathBuf};
    ///
    /// use adotenv::{exec::Shell, parse_strict, ExpandOptions};
    ///
    /// let mut origins = BTreeMap::new();
    /// origins.insert("api/.env".to_string(), PathBuf::from("/srv/app/api"));
    /// let shell = Shell::with_origins("/srv/app", origins);
    ///
    /// // This command runs in /srv/app/api.
    /// let doc = parse_strict("REVISION=$(git rev-parse HEAD)\n")?;
    /// let resolved = doc.resolve_with_commands(
    ///     |_| None,
    ///     shell.callback(),
    ///     Some("api/.env"),
    ///     &ExpandOptions::dotenv_expand(),
    /// )?;
    /// assert_eq!(resolved.commands().len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn with_origins(directory: impl Into<PathBuf>, origins: BTreeMap<String, PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            origins,
        }
    }

    /// Returns a callback for
    /// [`Document::resolve_with_commands`](crate::Document::resolve_with_commands)
    /// that runs each command with [`Shell::run`].
    ///
    /// The callback borrows this `Shell`. See the [`Shell`] example.
    #[must_use = "callbacks are lazy and do nothing unless passed to resolution"]
    pub fn callback(&self) -> impl FnMut(CommandContext<'_>) -> Result<String, CommandError> + '_ {
        move |context| self.run(&context)
    }

    /// Runs one command and returns its standard output without one trailing
    /// newline.
    ///
    /// This is what [`Shell::callback`] calls for each command. Call it from
    /// your own callback to decide first which commands may run.
    ///
    /// # Errors
    ///
    /// Returns a [`CommandError`] when:
    ///
    /// - the working directory does not exist or cannot be used, or the shell
    ///   cannot start;
    /// - the command exits with a nonzero status or is ended by a signal; the
    ///   message includes its standard error;
    /// - its standard output is not valid UTF-8.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use adotenv::{exec::Shell, parse_strict, CommandError, ExpandOptions};
    ///
    /// let shell = Shell::new("/srv/app");
    /// let doc = parse_strict("REVISION=$(git rev-parse HEAD)\n")?;
    /// let resolved = doc.resolve_with_commands(
    ///     |_| None,
    ///     |context| {
    ///         if context.command.starts_with("git ") {
    ///             shell.run(&context)
    ///         } else {
    ///             Err(CommandError::new(format!("not allowed: {}", context.command)))
    ///         }
    ///     },
    ///     None,
    ///     &ExpandOptions::dotenv_expand(),
    /// )?;
    /// assert_eq!(resolved.commands().len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn run(&self, context: &CommandContext<'_>) -> Result<String, CommandError> {
        let directory = context
            .source
            .and_then(|source| self.origins.get(source))
            .unwrap_or(&self.directory);
        let output = Command::new(SHELL)
            .arg(SHELL_FLAG)
            .arg(context.command)
            .current_dir(directory)
            .output()
            .map_err(|error| {
                CommandError::new(format!(
                    "failed to run command in {}: {error}",
                    directory.display()
                ))
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CommandError::new(format!(
                "command failed with {}: {}",
                output.status,
                stderr.trim_end()
            )));
        }
        let mut stdout = String::from_utf8(output.stdout)
            .map_err(|_| CommandError::new("command output is not valid UTF-8"))?;
        if stdout.ends_with('\n') {
            stdout.pop();
            if stdout.ends_with('\r') {
                stdout.pop();
            }
        }
        Ok(stdout)
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::Shell;
    use crate::{parse_strict, ExpandOptions};

    #[test]
    fn runs_a_command_and_trims_one_trailing_newline() {
        let shell = Shell::new(".");
        let doc = parse_strict("A=$(printf 'hi\\n\\n')\n").unwrap();
        let resolved = doc
            .resolve_with_commands(
                |_| None,
                shell.callback(),
                Some("synthetic"),
                &ExpandOptions::dotenv_expand(),
            )
            .unwrap();
        assert_eq!(resolved.entries()[0].value.as_deref(), Some("hi\n"));
        assert_eq!(resolved.commands().len(), 1);
    }

    #[test]
    fn rejects_nonzero_status_and_missing_directories() {
        let shell = Shell::new(".");
        let error = shell
            .run(&crate::CommandContext {
                command: "exit 7",
                reference: 0..0,
                payload: 0..0,
                source: None,
            })
            .unwrap_err();
        assert!(error.message().contains("failed"));

        let shell = Shell::new("/definitely/not/a/real/adotenv-directory");
        let error = shell
            .run(&crate::CommandContext {
                command: "printf hi",
                reference: 0..0,
                payload: 0..0,
                source: None,
            })
            .unwrap_err();
        assert!(error.message().contains("failed"));
    }

    #[test]
    fn source_origins_select_an_explicit_directory() {
        let directory = std::env::temp_dir().join(format!(
            "adotenv-exec-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("thread")
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let mut origins = std::collections::BTreeMap::new();
        origins.insert("synthetic".to_string(), directory.clone());
        let shell = Shell::with_origins(".", origins);
        let output = shell
            .run(&crate::CommandContext {
                command: "pwd",
                reference: 0..0,
                payload: 0..0,
                source: Some("synthetic"),
            })
            .unwrap();
        assert_eq!(
            std::fs::canonicalize(output.trim()).unwrap(),
            std::fs::canonicalize(directory).unwrap()
        );
        std::fs::remove_dir_all(std::fs::canonicalize(output.trim()).unwrap()).ok();
    }
}
