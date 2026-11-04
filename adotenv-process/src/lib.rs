//! Sets values loaded by [`adotenv`] in the current process environment.
//!
//! [`adotenv::load`] reads `.env` files into a [`LoadedEnvironment`] but never
//! changes the process environment. This crate adds that step: [`apply`]. It is
//! a separate crate because changing the environment is unsafe. The
//! environment is shared by every thread of the process, and on some platforms
//! reading it while another thread writes it is undefined behavior. The core
//! `adotenv` crate forbids unsafe code.
//!
//! ```toml
//! [dependencies]
//! adotenv = { version = "1", features = ["load"] }
//! adotenv-process = "1"
//! ```
//!
//! The crate has no features of its own and needs Rust 1.81 or later.
//!
//! # Prefer passing values explicitly
//!
//! Most programs do not need to change their own environment. Read values
//! with [`LoadedEnvironment::get`], deserialize them with the `serde` feature
//! of `adotenv`, or pass them to a child process:
//!
//! ```no_run
//! use std::process::Command;
//!
//! use adotenv::load::load_cascade;
//!
//! let loaded = load_cascade("/srv/my-app", "production")?;
//! let status = Command::new("my-server").envs(loaded.values()).status()?;
//! # let _ = status;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Use [`apply`] only when code you do not control reads the process
//! environment, such as a library that calls [`std::env::var`].
//!
//! # Applying a loaded environment
//!
//! [`apply`] checks every key and value, then sets each loaded key. A loaded
//! key replaces a variable of the same name. Other variables are kept and
//! nothing is removed. Values are set as loaded: `${HOME}` is not expanded.
//! The changes cannot be rolled back.
//!
//! The caller must guarantee that no other thread reads or writes the process
//! environment while [`apply`] runs. In practice, call it at the start of
//! `main`, before starting threads or an async runtime.
//!
//! ```
//! use adotenv::load::load_cascade;
//!
//! # let dir = std::env::temp_dir().join(format!("adotenv-process-doc-crate-{}", std::process::id()));
//! # std::fs::create_dir_all(&dir)?;
//! # std::fs::write(dir.join(".env"), "APP_PORT=3000\n")?;
//! # std::fs::write(dir.join(".env.production"), "APP_PORT=8080\n")?;
//! let loaded = load_cascade(&dir, "production")?;
//!
//! // SAFETY: called at startup, before any other thread exists.
//! unsafe { adotenv_process::apply(&loaded)? };
//! assert_eq!(std::env::var("APP_PORT")?, "8080");
//! # std::fs::remove_dir_all(&dir)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! [`validate`] runs the same checks without changing anything, and is safe
//! to call from any thread. Both return an [`ApplyError`] for the first key or
//! value that the process environment cannot hold.
//!
//! `apply` takes a [`LoadedEnvironment`]. To set resolved values, resolve them
//! with [`LoadedEnvironment::resolve_contextual`] and set them yourself.

use std::{env, fmt};

use adotenv::load::LoadedEnvironment;

#[cfg(windows)]
const WINDOWS_VALUE_LIMIT: usize = 32_767;

/// A key or value that the process environment cannot hold.
///
/// Returned by [`validate`], and by [`apply`] before it changes anything. It
/// reports the first problem, in key order. These are the inputs for which
/// [`std::env::set_var`] may panic. The error names the key but never includes
/// the value.
///
/// # Examples
///
/// Python dotenv lets a quoted key contain `=`. With `'APP=PORT'=8080` in
/// `.env`:
///
/// ```
/// use adotenv::load::load_cascade_with;
/// use adotenv::Dialect;
/// use adotenv_process::{validate, ApplyError};
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-process-doc-ApplyError-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "'APP=PORT'=8080\n")?;
/// let loaded = load_cascade_with(&dir, "", Dialect::PYTHON_DOTENV)?;
/// match validate(&loaded) {
///     Err(ApplyError::InvalidKey { key }) => assert_eq!(key, "APP=PORT"),
///     other => panic!("expected an invalid key, got {other:?}"),
/// }
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApplyError {
    /// A key is empty.
    ///
    /// The `adotenv` loaders skip empty keys, so a loaded environment does not
    /// produce this error; the check is a safeguard.
    EmptyKey,
    /// A key contains NUL or `=`.
    InvalidKey {
        /// The key, as loaded.
        key: String,
    },
    /// A value contains NUL.
    InvalidValue {
        /// The key whose value contains NUL.
        key: String,
    },
    /// Two keys collide on a case-insensitive platform.
    ///
    /// Windows environment variable names are case-insensitive. This variant
    /// is returned only on Windows.
    DuplicateKey {
        /// The later key in sorted order.
        key: String,
    },
    /// A value is too long for the platform.
    ///
    /// Windows accepts at most 32,767 UTF-16 code units per value. This variant
    /// is returned only on Windows.
    ValueTooLong {
        /// The key whose value is too long.
        key: String,
        /// The platform limit.
        limit: usize,
    },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyKey => formatter.write_str("environment key is empty"),
            Self::InvalidKey { key } => {
                write!(formatter, "environment key contains NUL or '=': {key:?}")
            }
            Self::InvalidValue { key } => {
                write!(formatter, "environment value for {key:?} contains NUL")
            }
            Self::DuplicateKey { key } => {
                write!(
                    formatter,
                    "environment key collides case-insensitively: {key:?}"
                )
            }
            Self::ValueTooLong { key, limit } => {
                write!(
                    formatter,
                    "environment value for {key:?} exceeds {limit} UTF-16 code units"
                )
            }
        }
    }
}

impl std::error::Error for ApplyError {}

/// Checks that the process environment can hold every loaded key and value.
///
/// Runs the checks of [`apply`] without changing anything, so it is safe to
/// call from any thread, for example to report a bad `.env` file early. A key
/// must not be empty and must not contain NUL or `=`. A value must not contain
/// NUL. Keys are checked in sorted order.
///
/// # Errors
///
/// Returns the first problem found:
///
/// - [`ApplyError::EmptyKey`] if a key is empty.
/// - [`ApplyError::InvalidKey`] if a key contains NUL or `=`.
/// - [`ApplyError::InvalidValue`] if a value contains NUL.
/// - On Windows, [`ApplyError::DuplicateKey`] if two keys differ only by case,
///   and [`ApplyError::ValueTooLong`] if a value exceeds 32,767 UTF-16 code
///   units.
///
/// # Examples
///
/// With `PORT=8080` and a `TOKEN` value that contains a NUL byte in `.env`:
///
/// ```
/// use adotenv::load::load_cascade;
/// use adotenv_process::{validate, ApplyError};
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-process-doc-validate-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "PORT=8080\nTOKEN=abc\0def\n")?;
/// let loaded = load_cascade(&dir, "")?;
/// let error = validate(&loaded).unwrap_err();
/// assert_eq!(error, ApplyError::InvalidValue { key: "TOKEN".to_string() });
/// assert_eq!(error.to_string(), r#"environment value for "TOKEN" contains NUL"#);
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn validate(environment: &LoadedEnvironment) -> Result<(), ApplyError> {
    #[cfg(windows)]
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in environment.values() {
        if key.is_empty() {
            return Err(ApplyError::EmptyKey);
        }
        if key.bytes().any(|byte| byte == 0 || byte == b'=') {
            return Err(ApplyError::InvalidKey {
                key: key.to_string(),
            });
        }
        if value.as_bytes().contains(&0) {
            return Err(ApplyError::InvalidValue {
                key: key.to_string(),
            });
        }
        #[cfg(windows)]
        {
            if !seen.insert(key.to_lowercase()) {
                return Err(ApplyError::DuplicateKey {
                    key: key.to_string(),
                });
            }
            if value.encode_utf16().count() > WINDOWS_VALUE_LIMIT {
                return Err(ApplyError::ValueTooLong {
                    key: key.to_string(),
                    limit: WINDOWS_VALUE_LIMIT,
                });
            }
        }
    }
    Ok(())
}

/// Sets every loaded key to its loaded value in the current process
/// environment.
///
/// First runs [`validate`], so an environment it rejects changes nothing. Then
/// sets each key with [`std::env::set_var`], in sorted order. A loaded key
/// replaces a variable of the same name. Other variables are kept and nothing
/// is removed. Values are set as loaded, without expanding references such as
/// `${HOME}`. The changes cannot be rolled back.
///
/// Prefer passing [`LoadedEnvironment::values`] to a child process, or reading
/// the values directly, unless code you do not control reads the process
/// environment. See the [crate documentation](crate).
///
/// # Errors
///
/// Returns the [`ApplyError`] from [`validate`], before changing anything.
///
/// # Safety
///
/// No other thread may read or write the process environment while `apply`
/// runs. That includes [`std::env::var`] and C functions such as `getenv` and
/// `setenv` called by libraries. On some platforms such concurrent access is
/// undefined behavior. In practice, call `apply` at the start of `main`, before
/// starting threads or an async runtime.
///
/// # Examples
///
/// With `APP_PORT=8080` and `APP_HOST=0.0.0.0` in `.env`:
///
/// ```
/// use adotenv::load::load_cascade;
/// use adotenv_process::apply;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-process-doc-apply-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "APP_PORT=8080\nAPP_HOST=0.0.0.0\n")?;
/// let loaded = load_cascade(&dir, "")?;
///
/// // SAFETY: called at startup, before any other thread exists.
/// unsafe { apply(&loaded)? };
/// assert_eq!(std::env::var("APP_PORT")?, "8080");
/// assert_eq!(std::env::var("APP_HOST")?, "0.0.0.0");
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub unsafe fn apply(environment: &LoadedEnvironment) -> Result<(), ApplyError> {
    validate(environment)?;
    for (key, value) in environment.values() {
        unsafe { env::set_var(key, value) };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Mutex,
        },
    };

    use super::*;
    use adotenv::load::load_cascade;

    static NEXT: AtomicU64 = AtomicU64::new(0);
    /// The process environment is global; serialize tests that touch it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "adotenv-process-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn environment(key: &str, value: &str) -> (TempDir, LoadedEnvironment) {
        let dir = TempDir::new();
        fs::write(dir.path().join(".env"), format!("{key}={value}\n")).unwrap();
        let loaded = load_cascade(dir.path(), "").unwrap();
        (dir, loaded)
    }

    #[test]
    fn apply_sets_loaded_values_and_can_be_removed() {
        let _guard = ENV_LOCK.lock().unwrap();
        let key = format!(
            "ADOTENV_PROCESS_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let (_dir, loaded) = environment(&key, "value");
        unsafe { apply(&loaded).unwrap() };
        assert_eq!(std::env::var(&key).as_deref(), Ok("value"));
        std::env::remove_var(&key);
    }

    #[test]
    fn validation_rejects_nul_values_before_mutation() {
        let _guard = ENV_LOCK.lock().unwrap();
        let key = format!(
            "ADOTENV_PROCESS_BAD_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let (_dir, loaded) = environment(&key, "bad\0value");
        assert!(matches!(
            validate(&loaded),
            Err(ApplyError::InvalidValue { .. })
        ));
        assert!(matches!(
            unsafe { apply(&loaded) },
            Err(ApplyError::InvalidValue { .. })
        ));
        assert!(std::env::var_os(&key).is_none());
    }

    #[test]
    fn validation_rejects_nul_keys_before_mutation() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new();
        let key = "ADOTENV_PROCESS_NUL\0KEY";
        fs::write(dir.path().join(".env"), format!("{key}=value\n")).unwrap();
        let loaded =
            adotenv::load::load_files([dir.path().join(".env")], adotenv::Dialect::DOCKER_RAW)
                .unwrap();
        assert_eq!(loaded.get(key), Some("value"));
        assert!(matches!(
            validate(&loaded),
            Err(ApplyError::InvalidKey { ref key }) if key == "ADOTENV_PROCESS_NUL\0KEY"
        ));
        assert!(matches!(
            unsafe { apply(&loaded) },
            Err(ApplyError::InvalidKey { .. })
        ));
    }

    #[test]
    fn all_apply_errors_have_descriptive_messages() {
        assert_eq!(ApplyError::EmptyKey.to_string(), "environment key is empty");
        assert_eq!(
            ApplyError::InvalidKey { key: "A=B".into() }.to_string(),
            "environment key contains NUL or '=': \"A=B\""
        );
        assert_eq!(
            ApplyError::InvalidValue { key: "A".into() }.to_string(),
            "environment value for \"A\" contains NUL"
        );
        assert_eq!(
            ApplyError::DuplicateKey { key: "Path".into() }.to_string(),
            "environment key collides case-insensitively: \"Path\""
        );
        assert_eq!(
            ApplyError::ValueTooLong {
                key: "A".into(),
                limit: 32_767,
            }
            .to_string(),
            "environment value for \"A\" exceeds 32767 UTF-16 code units"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_apply_rejects_duplicates_before_mutation() {
        let _guard = ENV_LOCK.lock().unwrap();
        let key = format!(
            "ADOTENV_PROCESS_CASE_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let other = key.to_ascii_lowercase();
        let dir = TempDir::new();
        fs::write(dir.path().join(".env"), format!("{key}=one\n{other}=two\n")).unwrap();
        let loaded = load_cascade(dir.path(), "").unwrap();
        assert!(std::env::var_os(&key).is_none());
        assert!(std::env::var_os(&other).is_none());
        assert!(matches!(
            unsafe { apply(&loaded) },
            Err(ApplyError::DuplicateKey { .. })
        ));
        assert!(std::env::var_os(&key).is_none());
        assert!(std::env::var_os(&other).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn windows_accepts_the_value_length_boundary() {
        let dir = TempDir::new();
        let value = "x".repeat(32_767);
        fs::write(dir.path().join(".env"), format!("A={value}\n")).unwrap();
        let loaded = load_cascade(dir.path(), "").unwrap();
        assert!(validate(&loaded).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn windows_rejects_case_insensitive_duplicate_keys() {
        let dir = TempDir::new();
        fs::write(dir.path().join(".env"), "PATH=one\nPath=two\n").unwrap();
        let loaded = load_cascade(dir.path(), "").unwrap();
        assert!(matches!(
            validate(&loaded),
            Err(ApplyError::DuplicateKey { ref key }) if key.eq_ignore_ascii_case("path")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_rejects_too_long_values() {
        let dir = TempDir::new();
        let value = "x".repeat(32_768);
        fs::write(dir.path().join(".env"), format!("A={value}\n")).unwrap();
        let loaded = load_cascade(dir.path(), "").unwrap();
        assert!(matches!(
            validate(&loaded),
            Err(ApplyError::ValueTooLong {
                ref key,
                limit: 32_767
            }) if key == "A"
        ));
    }

    #[test]
    fn error_messages_include_the_key() {
        let error = ApplyError::InvalidKey { key: "A=B".into() };
        assert!(error.to_string().contains("A=B"));
    }
}
