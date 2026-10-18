//! Opt-in system-shell command callback for trusted files.
//!
//! This module is available only with the `exec` feature. It runs arbitrary
//! commands through the platform shell in an explicit working directory. Only
//! pass trusted files and review every command. No implicit host working
//! directory or environment lookup is added by the core resolver.

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

/// A shell callback with an explicit default directory and optional source map.
///
/// Commands run exactly as written. Standard output is returned with one
/// trailing newline removed (including the preceding carriage return for CRLF).
/// Nonzero status and non-UTF-8 output are errors.
#[derive(Debug, Clone)]
pub struct Shell {
    directory: PathBuf,
    origins: BTreeMap<String, PathBuf>,
}

impl Shell {
    /// Construct a callback with an explicit working directory.
    ///
    /// The directory is used for every command unless a source-specific origin
    /// is configured with [`Self::with_origins`]. An invalid directory is
    /// reported when a command runs; there is no host-cwd fallback.
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            origins: BTreeMap::new(),
        }
    }

    /// Construct a callback with a default directory and source-identity origins.
    ///
    /// A context whose `source` matches a map entry runs in that directory;
    /// otherwise the default directory is used. Source identity stays opaque in
    /// the core and is only matched here for explicit execution policy.
    #[must_use]
    pub fn with_origins(directory: impl Into<PathBuf>, origins: BTreeMap<String, PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            origins,
        }
    }

    /// Return a `resolve_with_commands` callback borrowing this configuration.
    #[must_use = "callbacks are lazy and do nothing unless passed to resolution"]
    pub fn callback(&self) -> impl FnMut(CommandContext<'_>) -> Result<String, CommandError> + '_ {
        move |context| self.run(&context)
    }

    /// Run one command and return its standard output.
    ///
    /// # Errors
    /// Returns [`CommandError`] for an invalid directory, spawn failure, nonzero
    /// status or non-UTF-8 standard output.
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
