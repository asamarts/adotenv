#![no_main]
#![forbid(unsafe_code)]

use adotenv::{
    parse_entries_compact_with_options, parse_strict_with_options,
    parse_values_compact_with_options, parse_values_with_options, parse_with_options, Dialect,
    LimitKind, ParseOptions, Severity,
};
use libfuzzer_sys::fuzz_target;

fn check(source: &str, dialect: Dialect, options: ParseOptions) {
    let (document, diagnostics) = parse_with_options(source, dialect, options);
    assert_eq!(document.source(), source);
    for diagnostic in &diagnostics {
        if let Some(limit) = diagnostic.limit() {
            assert_eq!(diagnostic.severity(), Severity::Error);
            if limit.kind() != LimitKind::Diagnostics {
                assert!(limit.observed() > limit.limit(), "{limit:?}");
            }
        }
    }
    let _ = parse_strict_with_options(source, dialect, options);
    let _ = parse_values_with_options(source, dialect, options);
    let _ = parse_values_compact_with_options(source, dialect, options);
    let _ = parse_entries_compact_with_options(source, dialect, options);
}

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    for dialect in [Dialect::PORTABLE, Dialect::COMPOSE] {
        for limit in [0usize, 1, 2, 4, 16, 64] {
            check(
                source,
                dialect,
                ParseOptions::default()
                    .with_max_input_bytes(limit)
                    .with_max_lines(limit)
                    .with_max_items(limit)
                    .with_max_diagnostics(limit),
            );
            check(
                source,
                dialect,
                ParseOptions::default().with_max_lines(limit),
            );
        }
    }
});
