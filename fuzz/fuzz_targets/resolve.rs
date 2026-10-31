#![no_main]
#![forbid(unsafe_code)]

use adotenv::{
    parse, parse_with, CommandContext, CommandError, CommandOutcome, Dialect, ExpandOptions,
    ExpandScope, LookupOrder, ResolvedDocument, ResolvedSegment, ResolvedSegmentKind, Unresolved,
    ValueOrigin,
};
use libfuzzer_sys::fuzz_target;

/// Count segments, nested parts included, and their text bytes.
fn check_segments(
    source: &str,
    document: &adotenv::Document<'_>,
    segments: &[ResolvedSegment],
    totals: &mut (usize, usize),
) {
    for segment in segments {
        totals.0 += 1;
        totals.1 += segment.value.len();
        match &segment.kind {
            ResolvedSegmentKind::Literal { span } => {
                assert!(source.get(span.clone()).is_some());
            }
            ResolvedSegmentKind::Variable {
                reference,
                origin,
                parts,
                ..
            } => {
                assert!(source.get(reference.clone()).is_some());
                match origin {
                    Some(ValueOrigin::File { entry, span, .. }) => {
                        assert!(document.entry(*entry).is_some());
                        assert!(source.get(span.clone()).is_some());
                    }
                    Some(ValueOrigin::Lookup) | None => {}
                    _ => {}
                }
                check_segments(source, document, parts, totals);
            }
            ResolvedSegmentKind::Unresolved { reference, .. } => {
                assert!(source.get(reference.clone()).is_some());
            }
            ResolvedSegmentKind::Command {
                reference,
                payload,
                source: identity,
            } => {
                assert!(source.get(reference.clone()).is_some());
                assert!(source.get(payload.clone()).is_some());
                if let Some(identity) = identity {
                    assert_eq!(identity, "fuzz-source");
                }
            }
            _ => {}
        }
    }
}

/// Check the invariants every successful resolution keeps, including that the
/// output stays inside the configured budget: every segment is charged at
/// least its own size plus its text.
fn check_resolved(
    source: &str,
    document: &adotenv::Document<'_>,
    resolved: &ResolvedDocument,
    options: &ExpandOptions,
) {
    let mut totals = (0usize, 0usize);
    for entry in resolved.entries() {
        if let Some(value) = &entry.value {
            let joined: String = entry
                .segments
                .iter()
                .map(|segment| segment.value.as_str())
                .collect();
            assert_eq!(&joined, value);
            assert!(value.len() <= options.max_value_bytes);
        } else {
            assert!(entry.segments.is_empty());
        }
        check_segments(source, document, &entry.segments, &mut totals);
    }
    let (segments, text) = totals;
    let charged = segments
        .saturating_mul(std::mem::size_of::<ResolvedSegment>())
        .saturating_add(text);
    assert!(charged <= options.max_output_bytes);
}

/// Output budget for long values: a long value of plain references fits in
/// it, and an input that fills it after filling the default budget above
/// stays well inside the time limit.
const LONG_OUTPUT_BYTES: usize = 16 << 20;

/// Small limits, including a small output budget, for the full policy matrix.
fn small(options: ExpandOptions) -> ExpandOptions {
    options
        .with_max_depth(8)
        .with_max_value_bytes(1024)
        .with_max_output_bytes(1 << 16)
}

fn lookup(key: &str) -> Option<String> {
    if key.starts_with('M') {
        None
    } else {
        Some(format!("lookup-{key}"))
    }
}

fn resolve_checked(source: &str, document: &adotenv::Document<'_>, options: &ExpandOptions) {
    if let Ok(resolved) = document.resolve(lookup, options) {
        check_resolved(source, document, &resolved, options);
    }
}

fn resolve_commands_checked(
    source: &str,
    document: &adotenv::Document<'_>,
    options: &ExpandOptions,
) {
    let callback = |context: CommandContext<'_>| {
        assert_eq!(context.source, Some("fuzz-source"));
        assert!(source.get(context.reference.clone()).is_some());
        assert!(source.get(context.payload.clone()).is_some());
        if context.command.contains("FAIL") {
            Err(CommandError::new("synthetic"))
        } else {
            Ok(format!("command-{}", context.command))
        }
    };
    match document.resolve_with_commands(lookup, callback, Some("fuzz-source"), options) {
        Ok(resolved) => {
            for call in resolved.commands() {
                assert!(source.get(call.reference.clone()).is_some());
                assert!(source.get(call.payload.clone()).is_some());
                assert_eq!(call.source.as_deref(), Some("fuzz-source"));
                match &call.outcome {
                    CommandOutcome::Output(_) | CommandOutcome::Error(_) => {}
                    _ => {}
                }
            }
            check_resolved(source, document, &resolved, options);
        }
        Err(failure) => {
            for call in &failure.commands {
                assert!(source.get(call.reference.clone()).is_some());
                assert!(source.get(call.payload.clone()).is_some());
                assert_eq!(call.source.as_deref(), Some("fuzz-source"));
            }
        }
    }
}

/// A long single-line value built from a short input.
struct Long {
    source: String,
    dialect: Dialect,
}

/// Expand an input of the form `~`, a size byte, a fragment of at most 64
/// bytes and an optional newline-separated rest into one long single-line
/// value `A=...`, followed by the rest on later lines. The fragment repeats to
/// about `((size >> 2) + 1) * 2048` bytes, so up to 64 KiB for an ASCII size
/// byte. Bit 0 of the size byte appends one closer per repetition, so a
/// fragment that opens a form nests one level per repetition; bit 1 selects
/// the Compose dialect instead of Portable. Short inputs then reach long
/// reference runs and deep nesting.
fn repeated(bytes: &[u8], source: &str) -> Option<Long> {
    let (&marker, &size) = (bytes.first()?, bytes.get(1)?);
    if marker != b'~' {
        return None;
    }
    let (head, rest) = source
        .get(2..)?
        .split_once('\n')
        .unwrap_or((source.get(2..)?, ""));
    if head.is_empty() || head.len() > 64 {
        return None;
    }
    let repetitions = ((usize::from(size) >> 2) + 1) * 2048 / head.len();
    let closer = if head.contains("$(") { ')' } else { '}' };
    let mut long = String::with_capacity(3 + (head.len() + 1) * repetitions + rest.len());
    long.push_str("A=");
    for _ in 0..repetitions {
        long.push_str(head);
    }
    if size & 1 == 1 {
        for _ in 0..repetitions {
            long.push(closer);
        }
    }
    long.push('\n');
    long.push_str(rest);
    let dialect = if size & 2 == 2 {
        Dialect::COMPOSE
    } else {
        Dialect::PORTABLE
    };
    Some(Long {
        source: long,
        dialect,
    })
}

fuzz_target!(|bytes: &[u8]| {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let (document, _) = parse(source);
    // Small limits reach every limit path quickly under every policy.
    for base in [ExpandOptions::portable(), ExpandOptions::compose()] {
        for scope in [ExpandScope::Sequential, ExpandScope::WholeFile] {
            for lookup_order in [LookupOrder::FileFirst, LookupOrder::CallerFirst] {
                for unresolved in [Unresolved::Keep, Unresolved::Error] {
                    let options = base
                        .with_scope(scope)
                        .with_lookup_order(lookup_order)
                        .with_unresolved(unresolved);
                    resolve_checked(source, &document, &small(options));
                }
            }
        }
    }
    for scope in [ExpandScope::Sequential, ExpandScope::WholeFile] {
        let options = small(ExpandOptions::dotenv_expand().with_scope(scope));
        resolve_commands_checked(source, &document, &options);
    }
    // One preset with its own limits, so a regression in the default output
    // budget, in output accounting or in resolution time exceeds the fuzzer's
    // memory or time limit, and successful results are checked against the
    // budget. Filling the default budget takes about a second under the
    // sanitizer, so each input runs only one preset.
    match bytes.len() % 3 {
        0 => resolve_checked(source, &document, &ExpandOptions::portable()),
        1 => resolve_checked(
            source,
            &document,
            &ExpandOptions::compose().with_scope(ExpandScope::WholeFile),
        ),
        _ => resolve_commands_checked(
            source,
            &document,
            &ExpandOptions::dotenv_expand().with_scope(ExpandScope::WholeFile),
        ),
    }

    if let Some(long) = repeated(bytes, source) {
        let (document, _) = parse_with(&long.source, long.dialect);
        // Every candidate lists each form that contains it, so the result
        // grows with the square of the nesting depth. Only `${` and `$(`
        // forms contain candidates, so inspect it only for values that open
        // few of them; resolution scans the rest with its depth limit.
        let openers = long.source.matches("${").count() + long.source.matches("$(").count();
        if openers <= 1024 {
            for entry in document.entries() {
                let _ = entry.references();
            }
        }
        let options = ExpandOptions::compose()
            .with_scope(ExpandScope::WholeFile)
            .with_max_output_bytes(LONG_OUTPUT_BYTES);
        resolve_checked(&long.source, &document, &options);
    }
});
