//! Explicit process-environment application for loaded `.env` values.
//!
//! This crate deliberately lives outside `adotenv`: the core forbids unsafe code
//! and never mutates the process environment. Callers must use [`apply`]
//! deliberately, after loading has completed and validation has succeeded.

use std::{env, fmt};

use adotenv::load::LoadedEnvironment;

/// Validation or application failure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApplyError {
    /// A key was empty.
    EmptyKey,
    /// A key contained a NUL or `=`.
    InvalidKey {
        /// Offending key.
        key: String,
    },
    /// A value contained a NUL.
    InvalidValue {
        /// Key whose value is invalid.
        key: String,
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
        }
    }
}

impl std::error::Error for ApplyError {}

/// Validate every loaded key and value without mutating the environment.
///
/// # Errors
/// Returns [`ApplyError`] when a key is empty or contains NUL/`=`, or a value
/// contains NUL.
pub fn validate(environment: &LoadedEnvironment) -> Result<(), ApplyError> {
    for (key, value) in environment.values() {
        if key.is_empty() {
            return Err(ApplyError::EmptyKey);
        }
        if key.bytes().any(|byte| byte == 0 || byte == b'=') {
            return Err(ApplyError::InvalidKey { key: key.clone() });
        }
        if value.as_bytes().contains(&0) {
            return Err(ApplyError::InvalidValue { key: key.clone() });
        }
    }
    Ok(())
}

/// Apply every loaded value to the current process environment.
///
/// This sets only the keys present in the loaded environment. It does not clear
/// or remove any existing process variables. Every key and value is validated
/// before the first mutation.
///
/// # Safety
/// The process environment is global and not thread-safe on all platforms. The
/// caller must ensure that no other thread is concurrently reading or writing
/// the process environment and must accept that process-environment changes
/// cannot be rolled back. Prefer consuming [`LoadedEnvironment::values`] as a
/// map unless process-global mutation is actually required.
///
/// # Errors
/// Returns [`ApplyError`] before changing anything when validation fails.
pub unsafe fn apply(environment: &LoadedEnvironment) -> Result<(), ApplyError> {
    validate(environment)?;
    for (key, value) in environment.values() {
        env::set_var(key, value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use adotenv::load::load_cascade;

    static NEXT: AtomicU64 = AtomicU64::new(0);

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
    fn error_messages_include_the_key() {
        let error = ApplyError::InvalidKey { key: "A=B".into() };
        assert!(error.to_string().contains("A=B"));
    }
}
