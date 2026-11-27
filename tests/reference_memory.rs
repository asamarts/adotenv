//! Peak memory of reference scanning, resolution and parsing.
//!
//! The resident-set high-water mark belongs to the whole process, so the
//! measurements serialize on a mutex.
#![cfg(target_os = "linux")]

use adotenv::{
    parse, parse_with, DiagnosticCode, Dialect, EditError, EditOptions, ExpandOptions, LimitKind,
    ResolveError,
};

/// The high-water mark belongs to the whole process, so only one measurement
/// may run at a time.
static MEMORY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Growth of the resident set, in bytes, while `run` executes. Callers hold
/// `MEMORY_LOCK`, including while they build their inputs. Memory that earlier
/// steps freed may be reused, so this is a lower bound on what `run` allocates;
/// memory that grows with the square of the nesting depth still exceeds the
/// limit below by far.
fn peak_growth(run: impl FnOnce()) -> usize {
    // Writing 5 resets the high-water mark to the current resident set. Without
    // the reset, only growth past the earlier peak is visible.
    let reset = std::fs::write("/proc/self/clear_refs", "5").is_ok();
    let before = status_kib(if reset { "VmRSS:" } else { "VmHWM:" });
    run();
    status_kib("VmHWM:").saturating_sub(before) * 1024
}

fn status_kib(field: &str) -> usize {
    let status = std::fs::read_to_string("/proc/self/status").expect("process status is readable");
    status
        .lines()
        .find_map(|line| line.strip_prefix(field))
        .and_then(|rest| rest.trim().strip_suffix("kB"))
        .and_then(|value| value.trim().parse().ok())
        .expect("process status reports the field")
}

#[test]
fn lookup_index_does_not_allocate_for_ignored_bare_keys() {
    const LIMIT: usize = 1 << 20;
    let _guard = MEMORY_LOCK.lock().unwrap();
    let source = "BARE\n".repeat(200_000);
    let (document, _) = parse_with(&source, Dialect::NODE_DOTENV);
    let growth = peak_growth(|| {
        assert_eq!(document.lookup_index().len(), 0);
    });
    assert!(
        growth < LIMIT,
        "the lookup index grew the resident set by {growth} bytes"
    );
}

#[test]
fn parsing_caps_repeated_diagnostics_within_the_budget() {
    const LIMIT: usize = 16 << 20;
    let _guard = MEMORY_LOCK.lock().unwrap();
    let source = format!("A={}\n", "#".repeat(1_000_000));
    let growth = peak_growth(|| {
        let (_, diagnostics) = parse(&source);
        assert_eq!(diagnostics.len(), 10_001);
        assert_eq!(
            diagnostics.last().unwrap().code(),
            DiagnosticCode::TooManyDiagnostics
        );
    });
    assert!(
        growth < LIMIT,
        "the bounded diagnostic list grew the resident set by {growth} bytes"
    );
}

#[test]
fn resolution_rejects_a_dense_scan_within_the_reference_budget() {
    const LIMIT: usize = 32 << 20;
    let _guard = MEMORY_LOCK.lock().unwrap();
    // Two candidates per repeat; the default budget stops the scan after
    // 65,536 of them even though the value is under 1 MB.
    let value = "${A:-$A}".repeat(100_000);
    let source = format!("A={value}\n");
    let (doc, _) = parse(&source);
    let growth = peak_growth(|| {
        assert!(matches!(
            doc.resolve(|_| None, &ExpandOptions::portable()),
            Err(ResolveError::TooManyReferences {
                key,
                limit: 65_536,
                ..
            }) if key == "A"
        ));
    });
    assert!(
        growth < LIMIT,
        "the bounded reference scan grew the resident set by {growth} bytes"
    );
}

#[test]
fn nested_and_unclosed_forms_use_memory_linear_in_value_length() {
    const LIMIT: usize = 64 << 20;
    let _guard = MEMORY_LOCK.lock().unwrap();
    // The 16 KiB values come first, so memory that grows with the square of
    // the nesting depth fails an assertion before the 64 KiB values would
    // need gigabytes.
    for bytes in [16 << 10, 64 << 10] {
        let unclosed = "$(".repeat(bytes / 2);
        let commands = format!("{}{}", "$(".repeat(bytes / 3), ")".repeat(bytes / 3));
        let defaults = format!("{}{}", "${A:-".repeat(bytes / 6), "}".repeat(bytes / 6));
        for (value, forms) in [
            (unclosed, bytes / 2),
            (commands, bytes / 3),
            (defaults, bytes / 6),
        ] {
            let source = format!("B=1\nA={value}\n");
            let (doc, _) = parse(&source);
            let growth = peak_growth(|| {
                doc.sort().unwrap();
            });
            assert!(
                growth < LIMIT,
                "sorting {forms} forms grew the resident set by {growth} bytes"
            );
            let growth = peak_growth(|| {
                let references = doc.unique("A").unwrap().references();
                assert_eq!(references.len(), forms);
            });
            assert!(
                growth < LIMIT,
                "references for {forms} forms grew the resident set by {growth} bytes"
            );
        }
    }
}

#[test]
fn resolution_depth_budget_bounds_delimiter_scan() {
    const LIMIT: usize = 8 << 20;
    let _guard = MEMORY_LOCK.lock().unwrap();
    for value in [
        "$(".repeat(100_000),
        "${".repeat(100_000),
        format!("${{{}", "$(".repeat(100_000)),
    ] {
        let source = format!("A={value}\n");
        let (doc, _) = parse(&source);
        let growth = peak_growth(|| {
            assert!(matches!(
                doc.resolve(
                    |_| None,
                    &ExpandOptions::portable()
                        .with_max_references(2)
                        .with_max_depth(64)
                ),
                Err(ResolveError::DepthExceeded { limit: 64, .. })
            ));
        });
        assert!(
            growth < LIMIT,
            "the delimiter scan grew the resident set by {growth} bytes"
        );
    }
}

#[test]
fn rename_with_references_verification_bounds_delimiter_scan() {
    const LIMIT: usize = 8 << 20;
    let _guard = MEMORY_LOCK.lock().unwrap();
    // In Python dotenv, `'X"'` is a valid quoted key. Rewriting `$A` to `${X"}`
    // inside `$(echo "$A" )` toggles quoting in the rewritten output and exposes
    // 100,000 trailing `(` delimiters; verification must still enforce
    // `EditOptions::max_references`.
    let source = format!(
        "A=1\nB=$(echo \"$A\" ) \" {}{}\n",
        "(".repeat(100_000),
        ")".repeat(100_000)
    );
    let (doc, _) = parse_with(&source, Dialect::PYTHON_DOTENV);
    let growth = peak_growth(|| {
        let err = doc
            .rename_with_references_with_options(
                "A",
                "X\"",
                EditOptions::new().with_max_references(2),
            )
            .unwrap_err();
        assert!(
            matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::References),
            "expected references limit error, got {err:?}"
        );
    });
    assert!(
        growth < LIMIT,
        "rename verification delimiter scan grew the resident set by {growth} bytes"
    );
}

#[test]
fn max_references_delimiter_depth_cap_is_order_invariant() {
    // Under max_references = 50, the documented delimiter depth cap is 50 * 2 + 2 = 102.
    // An 80-paren nested command has delimiter depth 81 (<= 102) and 1 reference, while
    // 40 `$X` references leave remaining = 10 (< 32). The delimiter depth cap must stay
    // derived from the configured limit (102), not from remaining (which would shrink to 66),
    // so both entry orders succeed identically in sort_with_options and
    // rename_with_references_with_options.
    let nested = format!("A=$(echo {}x{})\n", "(".repeat(80), ")".repeat(80));
    let refs = format!("B={}\n", "$X".repeat(40));
    let opts = EditOptions::new().with_max_references(50);

    let sorted_order = format!("X=1\n{nested}{refs}");
    let unsorted_order = format!("X=1\n{refs}{nested}");

    let sorted_doc = adotenv::parse_strict(&sorted_order).unwrap();
    let unsorted_doc = adotenv::parse_strict(&unsorted_order).unwrap();

    assert_eq!(
        sorted_doc.sort_with_options(opts).unwrap().as_str(),
        format!("{nested}X=1\n{refs}")
    );
    assert_eq!(
        unsorted_doc.sort_with_options(opts).unwrap().as_str(),
        format!("{nested}X=1\n{refs}")
    );

    let renamed_sorted = sorted_doc
        .rename_with_references_with_options("X", "Y", opts)
        .unwrap();
    let renamed_unsorted = unsorted_doc
        .rename_with_references_with_options("X", "Y", opts)
        .unwrap();
    assert_eq!(
        renamed_sorted.as_str(),
        format!("Y=1\n{nested}B={}\n", "$Y".repeat(40))
    );
    assert_eq!(
        renamed_unsorted.as_str(),
        format!("Y=1\nB={}\n{nested}", "$Y".repeat(40))
    );

    // Depth 103 (> 102) still fails cleanly with LimitKind::References.
    let too_deep = format!(
        "B={}\nA=$(echo {}x{})\n",
        "$X".repeat(40),
        "(".repeat(102),
        ")".repeat(102)
    );
    let too_deep_doc = adotenv::parse_strict(&too_deep).unwrap();
    assert!(matches!(
        too_deep_doc.sort_with_options(opts),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::References
    ));
}
