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

use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{CommandContext, CommandError};

const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_STDERR_BYTES: usize = 64 * 1024;
const WAIT_INTERVAL: Duration = Duration::from_millis(10);

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
/// - gets an empty standard input;
/// - has its standard output capped at 1 MiB and its standard error capped at
///   64 KiB by default;
/// - is stopped after 300 seconds by default.
///
/// [`Shell::with_max_output_bytes`], [`Shell::with_timeout`] and
/// [`Shell::without_timeout`] change those limits. Output beyond the stdout
/// cap, a timeout, a failure to start, a nonzero status or output that is not
/// UTF-8 fails with a [`CommandError`], and resolution stops with
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
    max_output_bytes: usize,
    timeout: Option<Duration>,
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
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            timeout: Some(DEFAULT_TIMEOUT),
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
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            timeout: Some(DEFAULT_TIMEOUT),
        }
    }

    /// Sets the maximum number of standard-output bytes kept.
    ///
    /// The default is 1 MiB. A command that writes more fails with a
    /// [`CommandError`]; it is stopped and its partial output is discarded.
    #[must_use]
    pub const fn with_max_output_bytes(mut self, value: usize) -> Self {
        self.max_output_bytes = value;
        self
    }

    /// Sets how long a command may run before it is stopped.
    ///
    /// The default is 300 seconds. A stopped command fails with a
    /// [`CommandError`].
    #[must_use]
    pub const fn with_timeout(mut self, value: Duration) -> Self {
        self.timeout = Some(value);
        self
    }

    /// Lets a command run without a timeout.
    ///
    /// The output cap still applies. Without a timeout, a command that never
    /// exits blocks resolution forever.
    #[must_use]
    pub const fn without_timeout(mut self) -> Self {
        self.timeout = None;
        self
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
    /// - its standard output exceeds [`Shell::with_max_output_bytes`];
    /// - it runs longer than [`Shell::with_timeout`];
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
        let mut child = Command::new(SHELL)
            .arg(SHELL_FLAG)
            .arg(context.command)
            .current_dir(directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                CommandError::new(format!(
                    "failed to run command in {}: {error}",
                    directory.display()
                ))
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CommandError::new("command standard output was not captured"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CommandError::new("command standard error was not captured"))?;

        let overflow = Arc::new(AtomicBool::new(false));
        let stdout_overflow = Arc::clone(&overflow);
        let cap = self.max_output_bytes;
        let (stdout_sender, stdout_receiver) = mpsc::channel();
        let (stderr_sender, stderr_receiver) = mpsc::channel();
        let stdout_reader = thread::spawn(move || {
            let _ = stdout_sender.send(read_limited(stdout, cap, Some(&stdout_overflow)));
        });
        let stderr_reader = thread::spawn(move || {
            let _ = stderr_sender.send(read_limited(stderr, MAX_STDERR_BYTES, None));
        });

        let status = wait_for_child(&mut child, self.timeout, &overflow, cap);
        let status = status?;
        let stdout = receive_reader(&stdout_receiver, "standard output")?;
        let stderr = receive_reader(&stderr_receiver, "standard error")?;
        // The reader handles only outlive a timeout or an over-limit command.
        drop(stdout_reader);
        drop(stderr_reader);
        if overflow.load(Ordering::Relaxed) {
            return Err(CommandError::new(format!(
                "command output exceeds {cap} bytes"
            )));
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr);
            return Err(CommandError::new(format!(
                "command failed with {}: {}",
                status,
                stderr.trim_end()
            )));
        }
        let mut stdout = String::from_utf8(stdout)
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

fn wait_for_child(
    child: &mut Child,
    timeout: Option<Duration>,
    overflow: &AtomicBool,
    cap: usize,
) -> Result<ExitStatus, CommandError> {
    let started = Instant::now();
    loop {
        if overflow.load(Ordering::Relaxed) {
            stop_child(child);
            return Err(CommandError::new(format!(
                "command output exceeds {cap} bytes"
            )));
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                stop_child(child);
                return Err(CommandError::new(format!(
                    "failed to wait for command: {error}"
                )));
            }
        }
        if let Some(timeout) = timeout {
            if started.elapsed() >= timeout {
                stop_child(child);
                return Err(CommandError::new(format!(
                    "command timed out after {} seconds",
                    timeout.as_secs_f64()
                )));
            }
        }
        thread::sleep(WAIT_INTERVAL);
    }
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_limited(
    mut reader: impl Read,
    cap: usize,
    overflow: Option<&Arc<AtomicBool>>,
) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(cap.min(8192));
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(read) => {
                let room = cap.saturating_sub(bytes.len());
                if room > 0 {
                    bytes.extend_from_slice(&buffer[..read.min(room)]);
                }
                if read > room {
                    if let Some(flag) = overflow {
                        flag.store(true, Ordering::Relaxed);
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn receive_reader(
    receiver: &Receiver<io::Result<Vec<u8>>>,
    stream: &str,
) -> Result<Vec<u8>, CommandError> {
    match receiver.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error)) => Err(CommandError::new(format!(
            "failed to read command {stream}: {error}"
        ))),
        Err(RecvTimeoutError::Timeout) => Err(CommandError::new(format!(
            "command {stream} remained open after it stopped"
        ))),
        Err(RecvTimeoutError::Disconnected) => Err(CommandError::new(format!(
            "command {stream} reader stopped"
        ))),
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

    fn context(command: &str) -> crate::CommandContext<'_> {
        crate::CommandContext {
            command,
            reference: 0..0,
            payload: 0..0,
            source: None,
        }
    }

    #[test]
    fn default_output_cap_allows_more_than_the_mutated_small_caps() {
        let shell = Shell::new(".");
        let output = shell.run(&context("printf '%4096s' ''")).unwrap();
        assert_eq!(output.len(), 4096);
    }

    #[test]
    fn stderr_and_reader_caps_count_bytes_exactly() {
        use std::io::Cursor;

        let input = vec![b'x'; super::MAX_STDERR_BYTES + 1];
        let bytes = super::read_limited(Cursor::new(input), super::MAX_STDERR_BYTES, None).unwrap();
        assert_eq!(bytes.len(), super::MAX_STDERR_BYTES);

        let overflow = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let input = vec![b'x'; 4];
        let bytes = super::read_limited(Cursor::new(input), 4, Some(&overflow)).unwrap();
        assert_eq!(bytes.len(), 4);
        assert!(!overflow.load(std::sync::atomic::Ordering::Relaxed));
        let input = vec![b'x'; 5];
        let bytes = super::read_limited(Cursor::new(input), 4, Some(&overflow)).unwrap();
        assert_eq!(bytes.len(), 4);
        assert!(overflow.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn stops_a_command_that_exceeds_the_output_cap() {
        let shell = Shell::new(".").with_max_output_bytes(4);
        let error = shell.run(&context("printf 'abcdef'")).unwrap_err();
        assert!(error.message().contains("exceeds 4 bytes"), "{error}");
    }

    #[test]
    fn stops_a_command_that_exceeds_the_timeout() {
        let shell = Shell::new(".").with_timeout(std::time::Duration::from_millis(100));
        let error = shell.run(&context("sleep 5")).unwrap_err();
        assert!(error.message().contains("timed out"), "{error}");
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
