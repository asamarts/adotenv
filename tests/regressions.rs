//! End-to-end regressions for the fixes since 1.1.0.

use adotenv::{
    parse_strict, parse_with, Dialect, ExpandOptions, FinalNewline, FormatOptions, ResolveError,
};

#[test]
fn compose_operands_follow_operand_grammar() {
    let doc = parse_strict("A=${B:-do'nt}\n").unwrap();
    let resolved = doc.resolve(|_| None, &ExpandOptions::compose()).unwrap();
    assert_eq!(resolved.entries()[0].value.as_deref(), Some("do'nt"));

    let doc = parse_strict("A=${B:-\\$C}\n").unwrap();
    let resolved = doc
        .resolve(
            |key| (key == "C").then(|| "see".to_string()),
            &ExpandOptions::compose(),
        )
        .unwrap();
    assert_eq!(resolved.entries()[0].value.as_deref(), Some("\\see"));

    let doc = parse_strict("A=${B:?can't continue}\n").unwrap();
    let error = doc
        .resolve(|_| None, &ExpandOptions::compose())
        .unwrap_err();
    assert!(matches!(
        error,
        ResolveError::Required {
            ref name,
            ref message,
            ..
        } if name == "B" && message == "can't continue"
    ));

    let doc = parse_strict("A=${B:-do'nt}\n").unwrap();
    let resolved = doc
        .resolve(|_| None, &ExpandOptions::dotenv_expand())
        .unwrap();
    assert_eq!(resolved.entries()[0].value.as_deref(), Some("do'nt"));
}

#[test]
fn final_newline_remove_is_idempotent() {
    let options = FormatOptions::default()
        .with_trim_trailing_whitespace(true)
        .with_final_newline(FinalNewline::Remove);
    for (source, expected) in [
        ("U=\"multi\nline\" \r", "U=\"multi\nline\""),
        ("# a comment  \r", "# a comment"),
        ("A=1  \r", "A=1"),
        ("A=\"x  \" \r", "A=\"x  \""),
        ("  \r", "  \r"),
    ] {
        let doc = parse_strict(source).unwrap();
        let once = doc.format(&options).unwrap();
        assert_eq!(once.as_str(), expected, "{source:?}");
        let (after, _) = parse_with(once.as_str(), Dialect::PORTABLE);
        assert!(
            after.format(&options).unwrap().is_empty(),
            "{source:?} changed on the second pass"
        );
    }

    let (compose, _) = parse_with("A=1\r\r", Dialect::COMPOSE);
    assert_eq!(compose.format(&options).unwrap().as_str(), "A=1\r\r");
}

#[cfg(feature = "load")]
mod loaded {
    use super::*;
    use adotenv::{
        load::{load_cascade_with_options, LoadOptions},
        ExpandScope, LineBreaks,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "adotenv-regression-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
        fn write(&self, content: &str) {
            fs::write(self.0.join(".env"), content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn whole_file() -> ExpandOptions {
        ExpandOptions::portable().with_scope(ExpandScope::WholeFile)
    }

    #[test]
    fn contextual_resolution_keeps_every_loaded_value() {
        let dir = TempDir::new();
        dir.write("A=x\r\r\nB=${A}z\n");
        let loaded =
            load_cascade_with_options(dir.path(), "", LoadOptions::full(Dialect::DOCKER_RAW))
                .unwrap();
        let contextual = loaded.resolve_contextual(|_| None, &whole_file()).unwrap();
        let decoded = loaded.resolve_decoded(|_| None, &whole_file()).unwrap();
        assert_eq!(contextual.values(), decoded.values());
        assert_eq!(contextual.get("A"), Some("x\r"));
        assert_eq!(contextual.get("B"), Some("x\rz"));

        // The same value shape in a custom LF-only dialect.
        let dir = TempDir::new();
        dir.write("A=x\r\r\nB=${A}z\n");
        let dialect = Dialect::PORTABLE.with_line_breaks(LineBreaks::Lf);
        let loaded = load_cascade_with_options(dir.path(), "", LoadOptions::full(dialect)).unwrap();
        assert_eq!(loaded.get("A"), Some("x\r"));
        let contextual = loaded.resolve_contextual(|_| None, &whole_file()).unwrap();
        assert_eq!(contextual.get("A"), Some("x\r"));
        assert_eq!(contextual.get("B"), Some("x\rz"));

        // Multiline quoted values survive the synthetic parse.
        let dir = TempDir::new();
        dir.write("A=\"one\ntwo\"\nB=${A}\n");
        let loaded =
            load_cascade_with_options(dir.path(), "", LoadOptions::full(Dialect::PORTABLE))
                .unwrap();
        let contextual = loaded.resolve_contextual(|_| None, &whole_file()).unwrap();
        let decoded = loaded.resolve_decoded(|_| None, &whole_file()).unwrap();
        assert_eq!(contextual.values(), decoded.values());
        assert_eq!(contextual.get("A"), Some("one\ntwo"));
        assert_eq!(contextual.get("B"), Some("one\ntwo"));

        // A BOM that belongs to a later key is not stripped from a synthetic
        // first line.
        let dir = TempDir::new();
        dir.write("# c\n\u{feff}B=1\nA=${B}\n");
        let loaded =
            load_cascade_with_options(dir.path(), "", LoadOptions::full(Dialect::PYTHON_DOTENV))
                .unwrap();
        let contextual = loaded.resolve_contextual(|_| None, &whole_file()).unwrap();
        let decoded = loaded.resolve_decoded(|_| None, &whole_file()).unwrap();
        assert_eq!(contextual.values(), decoded.values());
        assert_eq!(contextual.get("\u{feff}B"), Some("1"));
        assert_eq!(contextual.get("B"), None);
        assert_eq!(contextual.get("A"), Some("${B}"));
    }
}
