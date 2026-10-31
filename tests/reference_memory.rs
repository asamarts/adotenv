//! Peak memory of reference scanning and sorting for deeply nested values.
//!
//! The resident-set high-water mark belongs to the whole process, so this
//! binary holds a single test.
#![cfg(target_os = "linux")]

use adotenv::parse;

/// Growth of the resident set, in bytes, while `run` executes. Memory that
/// earlier steps freed may be reused, so this is a lower bound on what `run`
/// allocates; memory that grows with the square of the nesting depth still
/// exceeds the limit below by far.
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
fn nested_and_unclosed_forms_use_memory_linear_in_value_length() {
    const LIMIT: usize = 64 << 20;
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
