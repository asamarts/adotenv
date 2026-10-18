use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    load::{load_cascade, load_cascade_with, load_files, LoadError, SkipReason},
    Dialect,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "adotenv-load-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
    fn write(&self, name: &str, content: &str) {
        fs::write(self.path.join(name), content).unwrap();
    }
    fn write_bytes(&self, name: &str, content: &[u8]) {
        fs::write(self.path.join(name), content).unwrap();
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn cascade_order_origins_and_overrides_are_explicit() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\nB=base\n");
    dir.write(".env.local", "B=local\nC=local\n");
    dir.write(".env.production", "C=mode\nD=mode\n");
    dir.write(".env.production.local", "D=mode-local\n");

    let loaded = load_cascade(dir.path(), "production").unwrap();
    assert_eq!(loaded.get("A"), Some("base"));
    assert_eq!(loaded.get("B"), Some("local"));
    assert_eq!(loaded.get("C"), Some("mode"));
    assert_eq!(loaded.get("D"), Some("mode-local"));
    assert_eq!(loaded.entries().len(), 7);
    assert_eq!(loaded.overrides().len(), 3);
    assert_eq!(
        loaded
            .overrides()
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>(),
        ["B", "C", "D"]
    );
    assert_eq!(
        loaded.origin("D").unwrap().path,
        dir.path().join(".env.production.local")
    );
    assert_eq!(loaded.origin("D").unwrap().line, 1);
    assert_eq!(loaded.origin("A").unwrap().path, dir.path().join(".env"));
}

#[test]
fn missing_cascade_files_are_skipped() {
    let dir = TempDir::new();
    dir.write(".env", "A=1\n");
    let loaded = load_cascade(dir.path(), "production").unwrap();
    assert_eq!(loaded.get("A"), Some("1"));
    assert_eq!(loaded.entries().len(), 1);

    let empty = TempDir::new();
    let loaded = load_cascade(empty.path(), "production").unwrap();
    assert!(loaded.is_empty());
    assert_eq!(loaded.entries().len(), 0);
}

#[test]
fn explicit_file_load_requires_each_file() {
    let dir = TempDir::new();
    dir.write("one.env", "A=1\n");
    let loaded = load_files([dir.path().join("one.env")], Dialect::PORTABLE).unwrap();
    assert_eq!(loaded.get("A"), Some("1"));
    assert!(matches!(
        load_files([dir.path().join("missing.env")], Dialect::PORTABLE),
        Err(LoadError::Io { .. })
    ));
}

#[test]
fn invalid_syntax_invalidates_the_whole_load() {
    let dir = TempDir::new();
    dir.write(".env", "A=1\n");
    dir.write(".env.local", "BAD KEY=value\n");
    assert!(matches!(
        load_cascade(dir.path(), ""),
        Err(LoadError::Invalid { .. })
    ));
}

#[test]
fn invalid_utf8_is_reported_without_lossy_decoding() {
    let dir = TempDir::new();
    dir.write_bytes(".env", b"A=\xff\n");
    assert!(matches!(
        load_cascade(dir.path(), ""),
        Err(LoadError::InvalidUtf8 { .. })
    ));
}

#[test]
fn duplicate_and_bare_declarations_are_reported() {
    let dir = TempDir::new();
    dir.write(".env", "A=1\nA=2\nBARE\nX=3\n");
    let loaded = load_cascade(dir.path(), "").unwrap();
    assert_eq!(loaded.get("A"), Some("2"));
    assert_eq!(loaded.get("X"), Some("3"));
    assert_eq!(loaded.repeated().len(), 1);
    assert_eq!(loaded.repeated()[0].key, "A");
    assert_eq!(loaded.repeated()[0].origins.len(), 2);
    assert_eq!(loaded.skipped().len(), 1);
    assert_eq!(loaded.skipped()[0].key.as_deref(), Some("BARE"));
    assert_eq!(loaded.skipped()[0].reason, SkipReason::Bare);
}

#[test]
fn bare_empty_policy_loads_an_effective_empty_value() {
    let dir = TempDir::new();
    dir.write(".env", "A\nX=1\n");
    let dialect = Dialect {
        bare_keys: crate::BareKeys::Empty,
        ..Dialect::PORTABLE
    };
    let loaded = load_cascade_with(dir.path(), "", dialect).unwrap();
    assert_eq!(loaded.get("A"), Some(""));
    assert_eq!(loaded.get("X"), Some("1"));
}

#[test]
fn nonparticipating_declarations_are_skipped() {
    let dir = TempDir::new();
    dir.write(".env", "A\nX=1\n");
    let dialect = Dialect {
        bare_keys: crate::BareKeys::Ignore,
        ..Dialect::PORTABLE
    };
    let loaded = load_cascade_with(dir.path(), "", dialect).unwrap();
    assert_eq!(loaded.get("X"), Some("1"));
    assert!(loaded.skipped().iter().any(|skipped| {
        skipped.key.as_deref() == Some("A") && skipped.reason == SkipReason::NonParticipating
    }));
}

#[test]
fn invalid_modes_cannot_escape_the_cascade() {
    let dir = TempDir::new();
    for mode in ["..", "../prod", "prod/file", "prod\\file"] {
        assert!(matches!(
            load_cascade(dir.path(), mode),
            Err(LoadError::InvalidMode)
        ));
    }
}

#[test]
fn empty_mode_loads_only_base_and_local_files() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\n");
    dir.write(".env.local", "B=local\n");
    dir.write(".env.production", "C=mode\n");
    let loaded = load_cascade(dir.path(), "").unwrap();
    assert_eq!(loaded.get("A"), Some("base"));
    assert_eq!(loaded.get("B"), Some("local"));
    assert_eq!(loaded.get("C"), None);
}

#[test]
fn origin_spans_address_the_original_file() {
    let dir = TempDir::new();
    let source = "# comment\nKEY=value\n";
    dir.write(".env", source);
    let loaded = load_cascade(dir.path(), "").unwrap();
    let origin = loaded.origin("KEY").unwrap();
    assert_eq!(&source[origin.span.clone()], "value");
    assert_eq!(origin.line, 2);
}
