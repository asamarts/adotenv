use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    load::{
        load_cascade, load_cascade_values, load_cascade_values_with, load_cascade_with,
        load_cascade_with_options, load_files, load_files_with_options, load_values, LoadError,
        LoadOptions, SkipReason,
    },
    Dialect, ExpandOptions, ExpandScope, Unresolved,
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
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded.entries().len(), 1);

    let empty = TempDir::new();
    let loaded = load_cascade(empty.path(), "production").unwrap();
    assert!(loaded.is_empty());
    assert_eq!(loaded.len(), 0);
    assert_eq!(loaded.entries().len(), 0);
}

#[cfg(feature = "serde")]
#[test]
fn loaded_environment_deserializes_final_values_with_origins() {
    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Config {
        host: String,
        port: u16,
    }

    let dir = TempDir::new();
    dir.write(".env", "host=localhost\nport=8080\n");
    dir.write(".env.local", "port=9090\n");
    let loaded = load_cascade(dir.path(), "").unwrap();
    let config: Config = loaded.deserialize().unwrap();
    assert_eq!(
        config,
        Config {
            host: "localhost".to_string(),
            port: 9090,
        }
    );

    let invalid = TempDir::new();
    invalid.write(".env", "host=localhost\nport=not-a-number\n");
    let loaded = load_cascade(invalid.path(), "").unwrap();
    let error = loaded.deserialize::<Config>().unwrap_err();
    assert_eq!(error.line(), 2);
    assert_eq!(error.key(), Some("port"));
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
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>(),
        ["A", "X"]
    );
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

#[test]
fn loaded_resolution_uses_final_values_and_origins() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\n");
    dir.write(".env.local", "B=${A}-suffix\n");
    let loaded = load_cascade(dir.path(), "").unwrap();
    let options = ExpandOptions {
        scope: ExpandScope::WholeFile,
        ..ExpandOptions::portable()
    };
    let resolved = loaded.resolve_decoded(|_| None, &options).unwrap();
    assert_eq!(resolved.get("A"), Some("base"));
    assert_eq!(resolved.get("B"), Some("base-suffix"));
    assert_eq!(resolved.len(), 2);
    assert_eq!(
        resolved.origin("B").unwrap().path,
        dir.path().join(".env.local")
    );
    assert_eq!(resolved.origin("B").unwrap().line, 1);

    let resolved = loaded
        .resolve_decoded(
            |name| (name == "EXTERNAL").then(|| "outside".to_string()),
            &options,
        )
        .unwrap();
    assert_eq!(resolved.get("A"), Some("base"));
}

#[test]
fn loaded_resolution_reports_scope_and_origin_failures() {
    let dir = TempDir::new();
    dir.write(".env", "A=${MISSING}\n");
    let loaded = load_cascade(dir.path(), "").unwrap();

    let error = loaded
        .resolve_decoded(|_| None, &ExpandOptions::portable())
        .unwrap_err();
    assert!(error.message().contains("whole-file"));

    let options = ExpandOptions {
        scope: ExpandScope::WholeFile,
        unresolved: Unresolved::Error,
        ..ExpandOptions::portable()
    };
    let error = loaded.resolve_decoded(|_| None, &options).unwrap_err();
    assert_eq!(error.key(), Some("A"));
    assert_eq!(error.origin().unwrap().path, dir.path().join(".env"));
    assert_eq!(error.origin().unwrap().line, 1);
}

#[test]
fn contextual_resolution_preserves_quote_and_escape_context() {
    let dir = TempDir::new();
    dir.write(".env", "A='${B}'\nB=value\nC=\"\\$B\"\nD=${B}\n");
    let loaded = load_cascade(dir.path(), "").unwrap();
    let options = ExpandOptions {
        scope: ExpandScope::WholeFile,
        ..ExpandOptions::portable()
    };
    let resolved = loaded.resolve_contextual(|_| None, &options).unwrap();
    assert_eq!(resolved.get("A"), Some("${B}"));
    assert_eq!(resolved.get("B"), Some("value"));
    assert_eq!(resolved.get("C"), Some("$B"));
    assert_eq!(resolved.get("D"), Some("value"));
    assert_eq!(resolved.origin("D").unwrap().line, 4);
}

#[test]
fn compact_values_match_full_load_but_drop_provenance() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\nA=last\nB\nC=one\n");
    dir.write(".env.local", "B=local\nC=two\n");
    dir.write(".env.production", "D=mode\n");
    dir.write(".env.production.local", "D=mode-local\n");

    let full = load_cascade(dir.path(), "production").unwrap();
    let compact = load_cascade_values(dir.path(), "production").unwrap();
    assert_eq!(compact.values(), full.values());
    assert_eq!(compact.get("A"), Some("last"));
    assert_eq!(compact.get("B"), Some("local"));
    assert_eq!(compact.len(), 4);
    assert!(!compact.is_empty());

    dir.write(".env.local", "A\n");
    let full = load_cascade(dir.path(), "production").unwrap();
    let compact = load_cascade_values(dir.path(), "production").unwrap();
    assert_eq!(compact.values(), full.values());
    assert_eq!(compact.get("A"), Some("last"));

    let map = compact.into_values();
    assert_eq!(map.get("D").map(String::as_str), Some("mode-local"));
    let from_full = full.into_values();
    assert_eq!(from_full.values(), &map);
}

#[test]
fn compact_values_use_the_same_dialect_and_error_rules() {
    let dir = TempDir::new();
    let explicit = dir.path().join("explicit.env");
    fs::write(&explicit, "A=first\nA=last\nB\nC='literal $HOME'\n").unwrap();

    let empty = Dialect::PORTABLE.with_bare_keys(crate::BareKeys::Empty);
    let values = load_values([&explicit], empty).unwrap();
    assert_eq!(values.get("A"), Some("last"));
    assert_eq!(values.get("B"), Some(""));
    assert_eq!(values.get("C"), Some("literal $HOME"));

    let ignored = Dialect::PORTABLE.with_bare_keys(crate::BareKeys::Ignore);
    let values = load_values([&explicit], ignored).unwrap();
    assert_eq!(values.get("A"), Some("last"));
    assert!(!values.values().contains_key("B"));

    assert!(matches!(
        load_values([dir.path().join("missing.env")], Dialect::PORTABLE),
        Err(LoadError::Io { .. })
    ));
    dir.write("bad.env", "BAD KEY=value\n");
    assert!(matches!(
        load_values([dir.path().join("bad.env")], Dialect::PORTABLE),
        Err(LoadError::Invalid { .. })
    ));

    dir.write(".env", "A=cascade\n");
    let values = load_cascade_values_with(dir.path(), "", Dialect::PORTABLE).unwrap();
    assert_eq!(values.get("A"), Some("cascade"));
}

#[test]
fn projected_values_match_full_loader_across_dialects() {
    let dir = TempDir::new();
    let cases = [
        (
            "portable",
            Dialect::PORTABLE,
            "A=1\nA=2\nB\nEMPTY=\nQ=\"a\\n\"\nM='one\ntwo'\n",
        ),
        (
            "first",
            Dialect::PORTABLE.with_duplicates(crate::Duplicates::First),
            "A\nA=2\nB=1\nB\nC=3\nC=4\n",
        ),
        (
            "bare-empty",
            Dialect::PORTABLE.with_bare_keys(crate::BareKeys::Empty),
            "A\nA=2\nB=1\nB\n",
        ),
        (
            "bare-ignore",
            Dialect::PORTABLE.with_bare_keys(crate::BareKeys::Ignore),
            "A\nA=2\n",
        ),
        (
            "quoted-key",
            Dialect::PORTABLE.with_quoted_keys(true),
            "'A'=one\n'A'=two\n",
        ),
        (
            "node",
            Dialect::NODE_DOTENV,
            "A=1\nB=${A}\n__proto__=x\nC=\"line\\nnext\"\n",
        ),
        ("python", Dialect::PYTHON_DOTENV, "A=1\nB='x'\n"),
        ("compose", Dialect::COMPOSE, "A=1\nB=\"x\"\n"),
        ("docker", Dialect::DOCKER_RAW, "A=one two \n"),
    ];

    for (name, dialect, source) in cases {
        let path = dir.path().join(format!("{name}.env"));
        fs::write(&path, source).unwrap();
        let full = load_files([&path], dialect).unwrap();
        let compact = load_values([&path], dialect).unwrap();
        assert_eq!(compact.values(), full.values(), "case {name}");
    }
}

#[test]
fn projected_values_reject_the_same_invalid_sources() {
    let dir = TempDir::new();
    let cases = [
        ("bad-key", Dialect::PORTABLE, "A=1\nBAD KEY=2\n"),
        ("unterminated", Dialect::PORTABLE, "A=\"unterminated\n"),
        (
            "unknown-escape",
            Dialect::PORTABLE.with_unknown_escapes(crate::UnknownEscapes::Error),
            "A=\"\\q\"\n",
        ),
        (
            "forbidden-bare",
            Dialect::PORTABLE.with_bare_keys(crate::BareKeys::Error),
            "A\n",
        ),
        (
            "line-limit",
            Dialect::PORTABLE.with_line_byte_limit(Some(2)),
            "LONG=value\n",
        ),
        (
            "bom-error",
            Dialect::PORTABLE.with_bom(crate::BomPolicy::Error),
            "\u{feff}A=1\n",
        ),
    ];

    for (name, dialect, source) in cases {
        let path = dir.path().join(format!("{name}.env"));
        fs::write(&path, source).unwrap();
        assert!(
            matches!(load_files([&path], dialect), Err(LoadError::Invalid { .. })),
            "full loader accepted {name}"
        );
        assert!(
            matches!(
                load_values([&path], dialect),
                Err(LoadError::Invalid { .. })
            ),
            "projected loader accepted {name}"
        );
    }
}

#[test]
fn full_loader_options_drop_raw_tokens_or_reports() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\nA=last\nB\nC=one\n");
    dir.write(".env.local", "B=local\nC=two\n");

    let compact = LoadOptions::new(Dialect::PORTABLE)
        .with_raw_tokens(false)
        .with_reports(false);
    let loaded = load_cascade_with_options(dir.path(), "", compact).unwrap();
    assert_eq!(loaded.get("A"), Some("last"));
    assert_eq!(loaded.get("B"), Some("local"));
    assert!(!loaded.retains_raw_tokens());
    assert_eq!(loaded.entries(), []);
    assert_eq!(loaded.overrides(), []);
    assert_eq!(loaded.repeated(), []);
    assert_eq!(loaded.skipped(), []);
    assert!(loaded.origin("A").is_some());
    assert!(loaded
        .resolve_contextual(|_| None, &ExpandOptions::portable())
        .is_err());
    assert!(loaded
        .resolve_decoded(
            |_| None,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..ExpandOptions::portable()
            },
        )
        .is_ok());

    let no_reports = LoadOptions::new(Dialect::PORTABLE).with_reports(false);
    let loaded = load_cascade_with_options(dir.path(), "", no_reports).unwrap();
    assert!(loaded.retains_raw_tokens());
    assert_eq!(loaded.entries(), []);
    let options = ExpandOptions {
        scope: ExpandScope::WholeFile,
        ..ExpandOptions::portable()
    };
    assert!(loaded.resolve_contextual(|_| None, &options).is_ok());

    let no_raw = LoadOptions::new(Dialect::PORTABLE).with_raw_tokens(false);
    let loaded = load_cascade_with_options(dir.path(), "", no_raw).unwrap();
    assert!(!loaded.retains_raw_tokens());
    assert_ne!(loaded.entries(), []);
    assert!(loaded.origin("A").is_some());
    assert!(loaded
        .resolve_contextual(|_| None, &ExpandOptions::portable())
        .is_err());
    assert!(loaded
        .resolve_decoded(
            |_| None,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..ExpandOptions::portable()
            },
        )
        .is_ok());

    assert!(matches!(
        load_cascade_with_options(dir.path(), "bad/mode", compact),
        Err(LoadError::InvalidMode)
    ));

    let explicit = dir.path().join("explicit.env");
    fs::write(&explicit, "Z=explicit\n").unwrap();
    let loaded = load_files_with_options(
        [&explicit],
        LoadOptions::new(Dialect::PORTABLE).with_raw_tokens(false),
    )
    .unwrap();
    assert_eq!(loaded.get("Z"), Some("explicit"));
}
