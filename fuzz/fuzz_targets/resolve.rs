#![no_main]
#![forbid(unsafe_code)]

use adotenv::{
    parse, ExpandOptions, ExpandScope, LookupOrder, ResolvedSegment, ResolvedSegmentKind,
    Unresolved, ValueOrigin,
};
use libfuzzer_sys::fuzz_target;

fn check_segments(source: &str, document: &adotenv::Document<'_>, segments: &[ResolvedSegment]) {
    for segment in segments {
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
                check_segments(source, document, parts);
            }
            ResolvedSegmentKind::Unresolved { reference, .. } => {
                assert!(source.get(reference.clone()).is_some());
            }
            _ => {}
        }
    }
}

fuzz_target!(|bytes: &[u8]| {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let (document, _) = parse(source);
    let bases = [ExpandOptions::portable(), ExpandOptions::compose()];
    let lookup = |key: &str| {
        if key.starts_with('M') {
            None
        } else {
            Some(format!("lookup-{key}"))
        }
    };
    for base in bases {
        for scope in [ExpandScope::Sequential, ExpandScope::WholeFile] {
            for lookup_order in [LookupOrder::FileFirst, LookupOrder::CallerFirst] {
                for unresolved in [Unresolved::Keep, Unresolved::Error] {
                    let options = ExpandOptions {
                        scope,
                        lookup_order,
                        unresolved,
                        max_depth: 8,
                        max_value_bytes: 1024,
                        ..base
                    };
                    let Ok(resolved) = document.resolve(lookup, &options) else {
                        continue;
                    };
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
                        check_segments(source, &document, &entry.segments);
                    }
                }
            }
        }
    }
});
