use std::{
    collections::BTreeMap,
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

fn full_cascade(dir: &Path, mode: &str) -> crate::load::LoadedEnvironment {
    load_cascade_with_options(dir, mode, LoadOptions::full(Dialect::PORTABLE)).unwrap()
}

fn full_cascade_with(dir: &Path, mode: &str, dialect: Dialect) -> crate::load::LoadedEnvironment {
    load_cascade_with_options(dir, mode, LoadOptions::full(dialect)).unwrap()
}

fn full_values(loaded: &crate::load::LoadedEnvironment) -> BTreeMap<String, String> {
    loaded
        .values()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn cascade_order_origins_and_overrides_are_explicit() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\nB=base\n");
    dir.write(".env.local", "B=local\nC=local\n");
    dir.write(".env.production", "C=mode\nD=mode\n");
    dir.write(".env.production.local", "D=mode-local\n");

    let loaded = full_cascade(dir.path(), "production");
    assert_eq!(loaded.get("A"), Some("base"));
    assert_eq!(loaded.get("B"), Some("local"));
    assert_eq!(loaded.get("C"), Some("mode"));
    assert_eq!(loaded.get("D"), Some("mode-local"));
    assert_eq!(loaded.entries().count(), 7);
    assert_eq!(loaded.overrides().count(), 3);
    assert_eq!(
        loaded
            .overrides()
            .map(|entry| entry.key())
            .collect::<Vec<_>>(),
        ["B", "C", "D"]
    );
    assert_eq!(
        loaded.origin("D").unwrap().path(),
        dir.path().join(".env.production.local")
    );
    assert_eq!(loaded.origin("D").unwrap().line(), 1);
    assert_eq!(loaded.origin("A").unwrap().path(), dir.path().join(".env"));
}

#[test]
fn borrowed_entries_preserve_overridden_values_and_origins() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\n");
    dir.write(".env.local", "A=local\n");

    let loaded = full_cascade(dir.path(), "");
    assert_eq!(loaded.get("A"), Some("local"));
    let entries = loaded.entries().collect::<Vec<_>>();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key(), "A");
    assert_eq!(entries[0].value(), "base");
    assert_eq!(entries[1].key(), "A");
    assert_eq!(entries[1].value(), "local");
    assert_eq!(entries[0].origin().path(), dir.path().join(".env"));
    assert_eq!(entries[1].origin().path(), dir.path().join(".env.local"));
}

#[test]
fn default_loader_retains_values_origins_and_raw_tokens_without_reports() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\nB=one\n");
    let local = "A=local\nC=two\n";
    dir.write(".env.local", local);

    let loaded = load_cascade(dir.path(), "").unwrap();
    assert_eq!(loaded.get("A"), Some("local"));
    assert_eq!(loaded.get("B"), Some("one"));
    assert_eq!(loaded.get("C"), Some("two"));
    assert_eq!(loaded.len(), 3);
    assert_eq!(
        loaded.values().collect::<Vec<_>>(),
        [("A", "local"), ("B", "one"), ("C", "two")]
    );
    let origin = loaded.origin("A").unwrap();
    assert_eq!(&local[origin.span().clone()], "local");
    assert_eq!(origin.line(), 1);
    assert!(loaded.retains_raw_tokens());
    assert_eq!(loaded.entries().count(), 0);
    assert_eq!(loaded.overrides().count(), 0);
    assert_eq!(loaded.repeated().count(), 0);
    assert_eq!(loaded.skipped().count(), 0);
}

#[test]
fn missing_cascade_files_are_skipped() {
    let dir = TempDir::new();
    dir.write(".env", "A=1\n");
    let loaded = load_cascade(dir.path(), "production").unwrap();
    assert_eq!(loaded.get("A"), Some("1"));
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded.entries().count(), 0);
    assert_eq!(full_cascade(dir.path(), "production").entries().count(), 1);

    let empty = TempDir::new();
    let loaded = load_cascade(empty.path(), "production").unwrap();
    assert!(loaded.is_empty());
    assert_eq!(loaded.len(), 0);
    assert_eq!(loaded.entries().count(), 0);
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
    let loaded = full_cascade(dir.path(), "");
    assert_eq!(loaded.get("A"), Some("2"));
    assert_eq!(loaded.get("X"), Some("3"));
    assert_eq!(
        loaded
            .entries()
            .map(|entry| entry.key())
            .collect::<Vec<_>>(),
        ["A", "X"]
    );
    let repeated = loaded.repeated().collect::<Vec<_>>();
    assert_eq!(repeated.len(), 1);
    assert_eq!(repeated[0].key(), "A");
    assert_eq!(repeated[0].origins().count(), 2);
    let skipped = loaded.skipped().collect::<Vec<_>>();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].key(), Some("BARE"));
    assert_eq!(skipped[0].reason(), SkipReason::Bare);
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
    let loaded = full_cascade_with(dir.path(), "", dialect);
    assert_eq!(loaded.get("X"), Some("1"));
    assert!(loaded.skipped().any(|skipped| {
        skipped.key() == Some("A") && skipped.reason() == SkipReason::NonParticipating
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
    assert_eq!(&source[origin.span().clone()], "value");
    assert_eq!(origin.line(), 2);
    let second = loaded.origin("KEY").unwrap();
    assert_eq!(second.span(), origin.span());
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
        resolved.origin("B").unwrap().path(),
        dir.path().join(".env.local")
    );
    assert_eq!(resolved.origin("B").unwrap().line(), 1);

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
    assert_eq!(error.origin().unwrap().path(), dir.path().join(".env"));
    assert_eq!(error.origin().unwrap().line(), 1);
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
    assert_eq!(resolved.origin("D").unwrap().line(), 4);
}

#[test]
fn compact_values_match_default_load_but_drop_provenance() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\nA=last\nB\nC=one\n");
    dir.write(".env.local", "B=local\nC=two\n");
    dir.write(".env.production", "D=mode\n");
    dir.write(".env.production.local", "D=mode-local\n");

    let full = load_cascade(dir.path(), "production").unwrap();
    let values = load_cascade_values(dir.path(), "production").unwrap();
    assert_eq!(values.values(), &full_values(&full));
    assert_eq!(values.get("A"), Some("last"));
    assert_eq!(values.get("B"), Some("local"));
    assert_eq!(values.len(), 4);
    assert!(!values.is_empty());

    dir.write(".env.local", "A\n");
    let full = load_cascade(dir.path(), "production").unwrap();
    let values = load_cascade_values(dir.path(), "production").unwrap();
    assert_eq!(values.values(), &full_values(&full));
    assert_eq!(values.get("A"), Some("last"));

    let map = values.into_values();
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
        assert_eq!(compact.values(), &full_values(&full), "case {name}");
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
fn default_loader_matches_full_values_and_origins() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\nB=one\n");
    let local = "A=local\nC=two\n";
    dir.write(".env.local", local);

    let loaded = load_cascade(dir.path(), "").unwrap();
    assert_eq!(loaded.get("A"), Some("local"));
    assert_eq!(loaded.get("B"), Some("one"));
    assert_eq!(loaded.get("C"), Some("two"));
    assert_eq!(loaded.len(), 3);
    assert_eq!(
        loaded.values().collect::<Vec<_>>(),
        [("A", "local"), ("B", "one"), ("C", "two")]
    );
    assert_eq!(
        loaded.origin("A").unwrap().path(),
        dir.path().join(".env.local")
    );
    let origin = loaded.origin("A").unwrap();
    assert_eq!(&local[origin.span().clone()], "local");
    assert_eq!(origin.line(), 1);
    assert_eq!(loaded.into_values().get("A"), Some("local"));

    let explicit = dir.path().join("explicit.env");
    fs::write(&explicit, "D=four\n").unwrap();
    let loaded = load_files([&explicit], Dialect::PORTABLE).unwrap();
    assert_eq!(loaded.get("D"), Some("four"));

    let no_origins = load_cascade_with_options(
        dir.path(),
        "",
        LoadOptions::new(Dialect::PORTABLE).with_origins(false),
    )
    .unwrap();
    assert_eq!(no_origins.get("A"), Some("local"));
    assert!(no_origins.origin("A").is_none());
}

#[test]
fn full_loader_can_drop_the_origin_map() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\n");
    dir.write(".env.local", "A=local\nB=two\n");

    let options = LoadOptions::full(Dialect::PORTABLE).with_origins(false);
    let loaded = load_cascade_with_options(dir.path(), "", options).unwrap();
    assert_eq!(loaded.get("A"), Some("local"));
    assert_eq!(loaded.get("B"), Some("two"));
    assert!(loaded.origin("A").is_none());
    assert!(loaded.origin("B").is_none());
    assert_eq!(loaded.overrides().count(), 0);
    assert_ne!(loaded.entries().count(), 0);

    let options = LoadOptions::full(Dialect::PORTABLE)
        .with_origins(false)
        .with_reports(false);
    let loaded = load_cascade_with_options(dir.path(), "", options).unwrap();
    assert_eq!(loaded.get("A"), Some("local"));
    assert_eq!(loaded.entries().count(), 0);
    assert!(loaded.origin("A").is_none());
    assert!(loaded.retains_raw_tokens());
}

#[test]
fn full_loader_options_drop_raw_tokens_or_reports() {
    let dir = TempDir::new();
    dir.write(".env", "A=base\nA=last\nB\nC=one\n");
    dir.write(".env.local", "B=local\nC=two\n");

    let compact = LoadOptions::new(Dialect::PORTABLE).with_raw_tokens(false);
    let loaded = load_cascade_with_options(dir.path(), "", compact).unwrap();
    assert_eq!(loaded.get("A"), Some("last"));
    assert_eq!(loaded.get("B"), Some("local"));
    assert!(!loaded.retains_raw_tokens());
    assert_eq!(loaded.entries().count(), 0);
    assert_eq!(loaded.overrides().count(), 0);
    assert_eq!(loaded.repeated().count(), 0);
    assert_eq!(loaded.skipped().count(), 0);
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

    let no_reports = LoadOptions::full(Dialect::PORTABLE).with_reports(false);
    let loaded = load_cascade_with_options(dir.path(), "", no_reports).unwrap();
    assert!(loaded.retains_raw_tokens());
    assert_eq!(loaded.entries().count(), 0);
    let options = ExpandOptions {
        scope: ExpandScope::WholeFile,
        ..ExpandOptions::portable()
    };
    assert!(loaded.resolve_contextual(|_| None, &options).is_ok());

    let no_raw = LoadOptions::full(Dialect::PORTABLE).with_raw_tokens(false);
    let loaded = load_cascade_with_options(dir.path(), "", no_raw).unwrap();
    assert!(!loaded.retains_raw_tokens());
    assert_ne!(loaded.entries().count(), 0);
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

#[test]
fn report_views_expose_borrowed_keys_values_and_origins() {
    let dir = TempDir::new();
    dir.write(".env", "A=one\nBARE\nC=1\nC=2\n");
    dir.write(".env.local", "A=three\n");

    let loaded = full_cascade(dir.path(), "");
    let entries = loaded.entries().collect::<Vec<_>>();
    assert_eq!(
        entries
            .iter()
            .map(|entry| (entry.key(), entry.value()))
            .collect::<Vec<_>>(),
        [("A", "one"), ("C", "2"), ("A", "three")]
    );
    assert_eq!(entries[0].origin().path(), dir.path().join(".env"));
    assert_eq!(entries[2].origin().path(), dir.path().join(".env.local"));

    let overrides = loaded.overrides().collect::<Vec<_>>();
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].key(), "A");
    assert_eq!(overrides[0].previous().path(), dir.path().join(".env"));
    assert_eq!(overrides[0].previous().line(), 1);
    assert_eq!(
        overrides[0].replacement().path(),
        dir.path().join(".env.local")
    );

    let repeated = loaded.repeated().collect::<Vec<_>>();
    assert_eq!(repeated.len(), 1);
    assert_eq!(repeated[0].key(), "C");
    let repeated_origins = repeated[0].origins().collect::<Vec<_>>();
    assert_eq!(repeated_origins.len(), 2);
    assert_eq!(repeated_origins[0].path(), dir.path().join(".env"));
    assert_eq!(repeated_origins[0].line(), 3);
    assert_eq!(repeated_origins[1].line(), 4);

    let skipped = loaded.skipped().collect::<Vec<_>>();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].key(), Some("BARE"));
    assert_eq!(skipped[0].reason(), SkipReason::Bare);
    assert_eq!(skipped[0].origin().path(), dir.path().join(".env"));
}

#[test]
fn reports_can_retain_origins_when_the_final_origin_map_is_disabled() {
    let dir = TempDir::new();
    dir.write(".env", "A=one\nA=two\n");
    dir.write(".env.local", "A=three\n");

    let loaded = load_cascade_with_options(
        dir.path(),
        "",
        LoadOptions::full(Dialect::PORTABLE).with_origins(false),
    )
    .unwrap();
    assert!(loaded.origin("A").is_none());
    assert_eq!(loaded.overrides().count(), 0);
    let entries = loaded.entries().collect::<Vec<_>>();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].origin().path(), dir.path().join(".env"));
    let repeated = loaded.repeated().collect::<Vec<_>>();
    assert_eq!(repeated.len(), 1);
    assert_eq!(repeated[0].origins().count(), 2);

    let resolved = loaded
        .resolve_decoded(
            |_| None,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..ExpandOptions::portable()
            },
        )
        .unwrap();
    assert_eq!(resolved.get("A"), Some("three"));
    assert!(resolved.origin("A").is_none());
}

#[test]
fn retention_options_and_compact_views_are_complete() {
    let defaults = LoadOptions::default();
    assert_eq!(defaults, LoadOptions::new(Dialect::PORTABLE));
    let options = defaults
        .with_dialect(Dialect::NODE_DOTENV)
        .with_raw_tokens(false)
        .with_reports(true)
        .with_origins(false);
    assert_eq!(options.dialect, Dialect::NODE_DOTENV);
    assert!(!options.retain_raw_tokens);
    assert!(options.retain_reports);
    assert!(!options.retain_origins);

    let dir = TempDir::new();
    let empty = load_cascade_values(dir.path(), "").unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert!(empty.values().is_empty());
    assert_eq!(empty.get("missing"), None);
    assert!(empty.into_values().is_empty());

    dir.write(".env", "A=1\n");
    let values = load_cascade_values(dir.path(), "").unwrap();
    assert_eq!(values.len(), 1);
    assert!(!values.is_empty());
    assert_eq!(values.get("A"), Some("1"));
    assert_eq!(values.values().get("A").map(String::as_str), Some("1"));
    assert_eq!(values.into_values().get("A").map(String::as_str), Some("1"));
}

#[test]
fn load_errors_have_stable_display_and_sources() {
    let dir = TempDir::new();
    let missing = dir.path().join("missing.env");
    let io = load_files([&missing], Dialect::PORTABLE).unwrap_err();
    assert!(io.to_string().contains("failed to read"));
    assert!(std::error::Error::source(&io).is_some());

    dir.write_bytes(".env", b"A=\xff\n");
    let utf8 = load_cascade(dir.path(), "").unwrap_err();
    assert!(utf8.to_string().contains("is not valid UTF-8"));
    assert!(std::error::Error::source(&utf8).is_none());

    dir.write(".env", "BAD KEY=value\n");
    let invalid = load_cascade(dir.path(), "").unwrap_err();
    assert!(invalid.to_string().contains("invalid-key"));
    assert!(std::error::Error::source(&invalid).is_none());

    let mode = load_cascade(dir.path(), "bad\0mode").unwrap_err();
    assert_eq!(mode.to_string(), "mode must not contain path separators");
    assert!(std::error::Error::source(&mode).is_none());
}

#[test]
fn compact_cascade_rejects_invalid_modes_and_empty_inputs() {
    let dir = TempDir::new();
    for mode in ["..", "../prod", "prod/file", "prod\\file", "prod\0mode"] {
        assert!(matches!(
            load_cascade_values_with(dir.path(), mode, Dialect::PORTABLE),
            Err(LoadError::InvalidMode)
        ));
    }
    let empty = load_values(std::iter::empty::<PathBuf>(), Dialect::PORTABLE).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn resolved_environment_and_errors_expose_origins() {
    let dir = TempDir::new();
    dir.write(".env", "A=${MISSING}\n");
    let loaded = load_cascade(dir.path(), "").unwrap();

    let scope = loaded
        .resolve_decoded(|_| None, &ExpandOptions::portable())
        .unwrap_err();
    assert_eq!(scope.key(), None);
    assert!(scope.origin().is_none());
    assert!(scope.message().contains("whole-file"));
    assert!(!scope.to_string().contains(':'));

    let options = ExpandOptions {
        scope: ExpandScope::WholeFile,
        unresolved: Unresolved::Error,
        ..ExpandOptions::portable()
    };
    let error = loaded.resolve_decoded(|_| None, &options).unwrap_err();
    assert_eq!(error.key(), Some("A"));
    assert!(error.message().contains("MISSING"));
    let origin = error.origin().unwrap();
    assert_eq!(origin.path(), dir.path().join(".env"));
    assert_eq!(&"A=${MISSING}\n"[origin.span().clone()], "${MISSING}");
    assert_eq!(origin.line(), 1);
    assert!(error
        .to_string()
        .starts_with(&format!("{}:1: ", dir.path().join(".env").display())));

    let contextual = loaded.resolve_contextual(|_| None, &options).unwrap_err();
    assert!(contextual.message().contains("MISSING"));
    assert_eq!(contextual.key(), Some("A"));
    assert!(contextual.origin().is_some());

    dir.write(".env", "A=ok\n");
    let loaded = load_cascade(dir.path(), "").unwrap();
    let resolved = loaded.resolve_decoded(|_| None, &options).unwrap();
    assert_eq!(resolved.values().get("A").map(String::as_str), Some("ok"));
    assert_eq!(resolved.len(), 1);
    assert!(!resolved.is_empty());
    assert_eq!(resolved.get("missing"), None);
    assert!(resolved.origin("A").is_some());
}
