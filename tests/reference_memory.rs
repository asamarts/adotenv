//! Peak memory of reference scanning, resolution and parsing.
//!
//! The resident-set high-water mark belongs to the whole process, so the
//! measurements serialize on a mutex.
#![cfg(target_os = "linux")]

use adotenv::{parse, DiagnosticCode, ExpandOptions, ResolveError};

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
