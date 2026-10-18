use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use proptest::prelude::*;

use crate::{
    parse_strict, parse_with, BareKeys, Dialect, ExpandOptions, ExpandScope, LookupOrder,
    ReferenceOperator, ResolveError, ResolvedSegmentKind, Unresolved, ValueOrigin,
};

fn resolve(
    source: &str,
    lookup: impl Fn(&str) -> Option<String>,
    options: &ExpandOptions,
) -> Result<crate::ResolvedDocument, ResolveError> {
    parse_strict(source).unwrap().resolve(lookup, options)
}

fn value(document: &crate::ResolvedDocument, index: usize) -> Option<&str> {
    document.entries()[index].value.as_deref()
}

#[test]
fn sequential_resolution_uses_earlier_bindings() {
    let doc = parse_strict("A=base\nB=${A}-suffix\n").unwrap();
    let resolved = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&resolved, 0), Some("base"));
    assert_eq!(value(&resolved, 1), Some("base-suffix"));
    let segments = &resolved.entries()[1].segments;
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].value, "base");
    assert!(matches!(
        segments[0].kind,
        ResolvedSegmentKind::Variable {
            origin: Some(ValueOrigin::File { entry: 0, .. }),
            ..
        }
    ));
    assert_eq!(segments[1].value, "-suffix");
    assert!(matches!(
        segments[1].kind,
        ResolvedSegmentKind::Literal { .. }
    ));
}

#[test]
fn whole_file_scope_uses_final_bindings_and_detects_cycles() {
    let lookup = |_: &str| None;
    let doc = parse_strict("A=${B}\nB=value\n").unwrap();
    let sequential = doc.resolve(lookup, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&sequential, 0), Some("${B}"));
    let whole = doc
        .resolve(
            lookup,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..ExpandOptions::portable()
            },
        )
        .unwrap();
    assert_eq!(value(&whole, 0), Some("value"));

    let doc = parse_strict("A=base\nA=${A}x\n").unwrap();
    let sequential = doc.resolve(lookup, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&sequential, 1), Some("basex"));
    assert!(matches!(
        doc.resolve(
            lookup,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..ExpandOptions::portable()
            }
        ),
        Err(ResolveError::Cycle { .. })
    ));

    let doc = parse_strict("A=${A}x\n").unwrap();
    let unresolved = doc.resolve(lookup, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&unresolved, 0), Some("${A}x"));
    assert!(matches!(
        doc.resolve(
            lookup,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..ExpandOptions::portable()
            }
        ),
        Err(ResolveError::Cycle { .. })
    ));
}

#[test]
fn caller_first_does_not_resolve_a_shadowed_file_cycle() {
    let doc = parse_strict("A=${A}\n").unwrap();
    let options = ExpandOptions {
        scope: ExpandScope::WholeFile,
        lookup_order: LookupOrder::CallerFirst,
        ..ExpandOptions::portable()
    };
    let resolved = doc
        .resolve(|key| (key == "A").then(|| "caller".to_string()), &options)
        .unwrap();
    assert_eq!(value(&resolved, 0), Some("caller"));
    assert!(matches!(
        doc.resolve(
            |_| None,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..options
            }
        ),
        Err(ResolveError::Cycle { .. })
    ));
}

#[test]
fn effective_values_follow_participation_and_bare_policy() {
    let doc = parse_strict("A=base\nB=\n").unwrap();
    let resolved = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
    assert_eq!(
        resolved.entries()[0].effective_value.as_deref(),
        Some("base")
    );
    assert_eq!(resolved.entries()[1].effective_value.as_deref(), Some(""));

    let node = parse_with("__proto__=x\n", Dialect::NODE_DOTENV).0;
    let resolved = node.resolve(|_| None, &ExpandOptions::portable()).unwrap();
    assert_eq!(resolved.entries()[0].value.as_deref(), Some("x"));
    assert_eq!(resolved.entries()[0].effective_value, None);

    let mut empty = Dialect::PORTABLE;
    empty.bare_keys = BareKeys::Empty;
    let doc = parse_with("A\n", empty).0;
    let resolved = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
    assert_eq!(resolved.entries()[0].value, None);
    assert_eq!(resolved.entries()[0].effective_value.as_deref(), Some(""));
}

#[test]
fn caller_lookup_order_and_origins_are_explicit() {
    let mut map = BTreeMap::new();
    map.insert("A", "caller");
    let lookup = |key: &str| map.get(key).map(|value| (*value).to_string());
    let doc = parse_strict("A=file\nB=${A}\n").unwrap();
    let file_first = doc.resolve(lookup, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&file_first, 1), Some("file"));
    let caller_first = doc
        .resolve(
            lookup,
            &ExpandOptions {
                lookup_order: LookupOrder::CallerFirst,
                ..ExpandOptions::portable()
            },
        )
        .unwrap();
    assert_eq!(value(&caller_first, 1), Some("caller"));
    assert!(matches!(
        caller_first.entries()[1].segments[0].kind,
        ResolvedSegmentKind::Variable {
            origin: Some(ValueOrigin::Lookup),
            ..
        }
    ));

    let doc = parse_strict("A\nB=${A}\n").unwrap();
    let shadowed = doc.resolve(lookup, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&shadowed, 1), Some("${A}"));
    let caller = doc
        .resolve(
            lookup,
            &ExpandOptions {
                lookup_order: LookupOrder::CallerFirst,
                ..ExpandOptions::portable()
            },
        )
        .unwrap();
    assert_eq!(value(&caller, 1), Some("caller"));
}

#[test]
fn every_operator_uses_set_empty_and_unset_correctly() {
    let source = "SET=value\nEMPTY=\nA=${SET:-def}\nB=${EMPTY:-def}\nC=${MISSING:-def}\nD=${SET-def}\nE=${EMPTY-def}\nF=${MISSING-def}\nG=${SET:+alt}\nH=${EMPTY:+alt}\nI=${MISSING:+alt}\nJ=${SET+alt}\nK=${EMPTY+alt}\nL=${MISSING+alt}\n";
    let resolved = resolve(source, |_| None, &ExpandOptions::compose()).unwrap();
    let expected = [
        "value", "def", "def", "value", "", "def", "alt", "", "", "alt", "alt", "",
    ];
    for (index, expected) in expected.into_iter().enumerate() {
        assert_eq!(value(&resolved, index + 2), Some(expected));
    }
    let ResolvedSegmentKind::Variable {
        name,
        operator,
        origin: Some(ValueOrigin::File { entry, key, .. }),
        parts,
        ..
    } = &resolved.entries()[2].segments[0].kind
    else {
        panic!("SET should resolve to a file variable segment");
    };
    assert_eq!(name, "SET");
    assert_eq!(*operator, Some(ReferenceOperator::DefaultIfUnsetOrEmpty));
    assert_eq!(*entry, 0);
    assert_eq!(key, "SET");
    assert_eq!(parts[0].value, "value");
}

#[test]
fn required_operators_return_structured_errors() {
    let doc = parse_strict("SET=value\nEMPTY=\nA=${MISSING:?boom}\n").unwrap();
    let error = doc
        .resolve(|_| None, &ExpandOptions::compose())
        .unwrap_err();
    assert!(matches!(
        error,
        ResolveError::Required {
            ref key,
            ref name,
            ref message,
            ..
        } if key == "A" && name == "MISSING" && message == "boom"
    ));
    let doc = parse_strict("EMPTY=\nA=${EMPTY:?empty}\n").unwrap();
    assert!(matches!(
        doc.resolve(|_| None, &ExpandOptions::compose()),
        Err(ResolveError::Required { .. })
    ));
    let doc = parse_strict("SET=value\nA=${SET:?unused}\n").unwrap();
    assert_eq!(
        value(
            &doc.resolve(|_| None, &ExpandOptions::compose()).unwrap(),
            1
        ),
        Some("value")
    );
}

#[test]
fn nested_defaults_are_conditional_and_expanded() {
    let source = "A=${MISSING:-${B:-fallback}}\nB=beta\n";
    let sequential = resolve(source, |_| None, &ExpandOptions::compose()).unwrap();
    assert_eq!(value(&sequential, 0), Some("fallback"));
    let whole = resolve(
        source,
        |_| None,
        &ExpandOptions {
            scope: ExpandScope::WholeFile,
            ..ExpandOptions::compose()
        },
    )
    .unwrap();
    assert_eq!(value(&whole, 0), Some("beta"));

    let source = "SET=value\nA=${SET:-${MISSING:?unused}}\n";
    let resolved = resolve(source, |_| None, &ExpandOptions::compose()).unwrap();
    assert_eq!(value(&resolved, 1), Some("value"));

    let source = "MISSING2=new\nA=${MISSING:-$MISSING2}\n";
    let resolved = resolve(source, |_| None, &ExpandOptions::compose()).unwrap();
    assert_eq!(value(&resolved, 1), Some("new"));
}

#[test]
fn unresolved_policy_controls_missing_references() {
    let doc = parse_strict("A=${MISSING}\n").unwrap();
    let keep = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&keep, 0), Some("${MISSING}"));
    assert!(matches!(
        keep.entries()[0].segments[0].kind,
        ResolvedSegmentKind::Unresolved { .. }
    ));
    let empty = doc
        .resolve(
            |_| None,
            &ExpandOptions {
                unresolved: Unresolved::Empty,
                ..ExpandOptions::portable()
            },
        )
        .unwrap();
    assert_eq!(value(&empty, 0), Some(""));
    assert_eq!(empty.entries()[0].segments, Vec::new());
    assert!(matches!(
        doc.resolve(
            |_| None,
            &ExpandOptions {
                unresolved: Unresolved::Error,
                ..ExpandOptions::portable()
            }
        ),
        Err(ResolveError::Unresolved { .. })
    ));
}

#[test]
fn quote_escape_backtick_and_command_eligibility_is_honored() {
    let single = "A='${B}'\n";
    let literal = resolve(
        single,
        |key| (key == "B").then(|| "b".to_string()),
        &ExpandOptions::portable(),
    )
    .unwrap();
    assert_eq!(value(&literal, 0), Some("${B}"));
    let quote_rules = ExpandOptions {
        rules: crate::ReferenceRules {
            single_quotes: true,
            ..crate::ReferenceRules::PORTABLE
        },
        ..ExpandOptions::portable()
    };
    let expanded = resolve(
        single,
        |key| (key == "B").then(|| "b".to_string()),
        &quote_rules,
    )
    .unwrap();
    assert_eq!(value(&expanded, 0), Some("b"));

    let escaped = "A=\"\\${B}\"\n";
    let literal = resolve(
        escaped,
        |key| (key == "B").then(|| "b".to_string()),
        &ExpandOptions::portable(),
    )
    .unwrap();
    assert_eq!(value(&literal, 0), Some("${B}"));
    let escape_rules = ExpandOptions {
        rules: crate::ReferenceRules {
            escaped_dollar: true,
            ..crate::ReferenceRules::PORTABLE
        },
        ..ExpandOptions::portable()
    };
    let expanded = resolve(
        escaped,
        |key| (key == "B").then(|| "b".to_string()),
        &escape_rules,
    )
    .unwrap();
    assert_eq!(value(&expanded, 0), Some("b"));

    let doc = parse_with("A=`${B}`\n", Dialect::NODE_DOTENV).0;
    let literal = doc
        .resolve(
            |key| (key == "B").then(|| "b".to_string()),
            &ExpandOptions::portable(),
        )
        .unwrap();
    assert_eq!(literal.entries()[0].value.as_deref(), Some("${B}"));
    let backtick_rules = ExpandOptions {
        rules: crate::ReferenceRules {
            backtick_quotes: true,
            ..crate::ReferenceRules::PORTABLE
        },
        ..ExpandOptions::portable()
    };
    let expanded = doc
        .resolve(|key| (key == "B").then(|| "b".to_string()), &backtick_rules)
        .unwrap();
    assert_eq!(expanded.entries()[0].value.as_deref(), Some("b"));

    let command = parse_strict("A=$(echo $B)\n").unwrap();
    let resolved = command
        .resolve(
            |key| (key == "B").then(|| "b".to_string()),
            &ExpandOptions::dotenv_expand(),
        )
        .unwrap();
    assert_eq!(value(&resolved, 0), Some("$(echo $B)"));
}

#[test]
fn bare_duplicate_and_nonparticipating_bindings_follow_lookup_rules() {
    let source = "A=one\nA=two\nB=${A}\n";
    let resolved = resolve(source, |_| None, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&resolved, 2), Some("two"));

    let source = "A\nB=${A}\n";
    let bare = resolve(source, |_| None, &ExpandOptions::portable()).unwrap();
    assert_eq!(value(&bare, 1), Some("${A}"));
    let mut empty = Dialect::PORTABLE;
    empty.bare_keys = BareKeys::Empty;
    let doc = parse_with(source, empty).0;
    let resolved = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
    assert_eq!(resolved.entries()[1].value.as_deref(), Some(""));

    let node = parse_with("__proto__=x\nA=${__proto__}\n", Dialect::NODE_DOTENV).0;
    let resolved = node
        .resolve(
            |key| (key == "__proto__").then(|| "caller".to_string()),
            &ExpandOptions::portable(),
        )
        .unwrap();
    assert_eq!(resolved.entries()[1].value.as_deref(), Some("caller"));
}

#[test]
fn limits_and_cycles_are_structured_errors() {
    let doc = parse_strict("A=${B}\nB=${C}\nC=value\n").unwrap();
    let resolved = doc
        .resolve(
            |_| None,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..ExpandOptions::portable()
            },
        )
        .unwrap();
    assert_eq!(value(&resolved, 0), Some("value"));
    let cycle = parse_strict("A=${B}\nB=${A}\n").unwrap();
    assert!(matches!(
        cycle.resolve(
            |_| None,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                ..ExpandOptions::portable()
            }
        ),
        Err(ResolveError::Cycle { .. })
    ));
    let large = parse_strict("A=${B}\nB=abcdef\n").unwrap();
    assert!(matches!(
        large.resolve(
            |_| None,
            &ExpandOptions {
                max_value_bytes: 3,
                ..ExpandOptions::portable()
            }
        ),
        Err(ResolveError::ValueTooLarge { .. })
    ));
    let deep = parse_strict("A=${B}\nB=${C}\nC=${D}\nD=x\n").unwrap();
    assert!(matches!(
        deep.resolve(
            |_| None,
            &ExpandOptions {
                scope: ExpandScope::WholeFile,
                max_depth: 1,
                ..ExpandOptions::portable()
            }
        ),
        Err(ResolveError::DepthExceeded { .. })
    ));
}

#[test]
fn provenance_preserves_nested_file_parts() {
    let source = "A=x\nB=${A}\nC=${B}\n";
    let resolved = resolve(source, |_| None, &ExpandOptions::portable()).unwrap();
    let c = &resolved.entries()[2].segments[0];
    let ResolvedSegmentKind::Variable {
        origin: Some(ValueOrigin::File { entry: 1, .. }),
        parts,
        ..
    } = &c.kind
    else {
        panic!("C should reference a file binding");
    };
    assert_eq!(parts.len(), 1);
    assert!(matches!(
        parts[0].kind,
        ResolvedSegmentKind::Variable {
            origin: Some(ValueOrigin::File { entry: 0, .. }),
            ..
        }
    ));
    assert_eq!(parts[0].value, "x");
}

#[test]
fn appended_tail_references_are_resolved_as_unquoted_text() {
    let mut dialect = Dialect::PORTABLE;
    dialect.quoted_tail = crate::QuotedTail::Append;
    let doc = parse_with("A=\"$B\"$C\n", dialect).0;
    let resolved = doc
        .resolve(
            |key| match key {
                "B" => Some("b".to_string()),
                "C" => Some("c".to_string()),
                _ => None,
            },
            &ExpandOptions::portable(),
        )
        .unwrap();
    assert_eq!(resolved.entries()[0].value.as_deref(), Some("bc"));
}

#[test]
fn caller_values_are_literal_and_never_reexpanded() {
    let doc = parse_strict("A=${B}\n").unwrap();
    let resolved = doc
        .resolve(
            |key| (key == "B").then(|| "$C".to_string()),
            &ExpandOptions::compose(),
        )
        .unwrap();
    assert_eq!(value(&resolved, 0), Some("$C"));
    assert!(matches!(
        resolved.entries()[0].segments[0].kind,
        ResolvedSegmentKind::Variable {
            origin: Some(ValueOrigin::Lookup),
            ..
        }
    ));
}

#[test]
fn compose_dollar_escape_is_observable_and_optional() {
    let doc = parse_with("A=$$B\n", Dialect::COMPOSE).0;
    let literal = doc
        .resolve(
            |key| (key == "B").then(|| "b".to_string()),
            &ExpandOptions::compose(),
        )
        .unwrap();
    assert_eq!(literal.entries()[0].value.as_deref(), Some("$$B"));
    let rules = crate::ReferenceRules {
        escaped_dollar: true,
        ..crate::ReferenceRules::COMPOSE
    };
    let expanded = doc
        .resolve(
            |key| (key == "B").then(|| "b".to_string()),
            &ExpandOptions {
                rules,
                ..ExpandOptions::compose()
            },
        )
        .unwrap();
    assert_eq!(expanded.entries()[0].value.as_deref(), Some("$b"));
}

#[test]
fn command_callback_receives_raw_text_and_records_calls() {
    let source = "A=$(echo $B)\n";
    let doc = parse_strict(source).unwrap();
    let mut seen = Vec::new();
    let resolved = doc
        .resolve_with_commands(
            |_| None,
            |context| {
                seen.push((
                    context.command.to_string(),
                    context.reference.clone(),
                    context.payload.clone(),
                    context.source.map(str::to_string),
                ));
                Ok("output".to_string())
            },
            Some("synthetic.env"),
            &ExpandOptions {
                rules: crate::ReferenceRules::DOTENV_EXPAND,
                ..ExpandOptions::portable()
            },
        )
        .unwrap();
    assert_eq!(value(&resolved, 0), Some("output"));
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "echo $B");
    assert_eq!(&source[seen[0].1.clone()], "$(echo $B)");
    assert_eq!(&source[seen[0].2.clone()], "echo $B");
    assert_eq!(seen[0].3.as_deref(), Some("synthetic.env"));
    assert_eq!(resolved.commands().len(), 1);
    assert_eq!(
        resolved.commands()[0].outcome,
        crate::CommandOutcome::Output("output".into())
    );
    assert!(matches!(
        resolved.entries()[0].segments[0].kind,
        ResolvedSegmentKind::Command { .. }
    ));
}

#[test]
fn command_failures_retain_prior_calls() {
    let doc = parse_strict("A=$(first)\nB=$(second)\n").unwrap();
    let failure = doc
        .resolve_with_commands(
            |_| None,
            |context| {
                if context.command == "second" {
                    Err(crate::CommandError::new("synthetic failure"))
                } else {
                    Ok("first-output".to_string())
                }
            },
            None,
            &ExpandOptions {
                rules: crate::ReferenceRules::DOTENV_EXPAND,
                ..ExpandOptions::portable()
            },
        )
        .unwrap_err();
    assert!(matches!(
        failure.error,
        ResolveError::Command {
            ref key,
            ref message,
            ..
        } if key == "B" && message == "synthetic failure"
    ));
    assert_eq!(failure.commands.len(), 2);
    assert_eq!(
        failure.commands[0].outcome,
        crate::CommandOutcome::Output("first-output".into())
    );
    assert_eq!(
        failure.commands[1].outcome,
        crate::CommandOutcome::Error("synthetic failure".into())
    );
}

#[test]
fn commands_are_literal_without_a_callback_and_respect_rules() {
    let doc = parse_strict("A=$(echo hi)\n").unwrap();
    let literal = doc
        .resolve(|_| None, &ExpandOptions::dotenv_expand())
        .unwrap();
    assert_eq!(value(&literal, 0), Some("$(echo hi)"));
    assert_eq!(literal.commands().len(), 0);

    let mut invoked = 0;
    let _ = doc
        .resolve_with_commands(
            |_| None,
            |_| {
                invoked += 1;
                Ok("output".to_string())
            },
            None,
            &ExpandOptions::compose(),
        )
        .unwrap();
    assert_eq!(invoked, 0);
}

#[test]
fn conditional_command_arguments_are_only_invoked_when_chosen() {
    let source = "SET=x\nA=${SET:-$(unused)}\nB=${MISSING:-$(used)}\n";
    let doc = parse_strict(source).unwrap();
    let mut invoked = Vec::new();
    let resolved = doc
        .resolve_with_commands(
            |_| None,
            |context| {
                invoked.push(context.command.to_string());
                Ok("command-output".to_string())
            },
            None,
            &ExpandOptions::dotenv_expand(),
        )
        .unwrap();
    assert_eq!(value(&resolved, 1), Some("x"));
    assert_eq!(value(&resolved, 2), Some("command-output"));
    assert_eq!(invoked, ["used"]);
    assert_eq!(resolved.commands().len(), 1);
}

#[test]
fn command_calls_survive_a_later_non_command_error() {
    let doc = parse_strict("A=$(cmd)\nB=${MISSING:?required}\n").unwrap();
    let failure = doc
        .resolve_with_commands(
            |_| None,
            |_| Ok("output".to_string()),
            None,
            &ExpandOptions::dotenv_expand(),
        )
        .unwrap_err();
    assert!(matches!(failure.error, ResolveError::Required { .. }));
    assert_eq!(failure.commands.len(), 1);
    assert_eq!(
        failure.commands[0].outcome,
        crate::CommandOutcome::Output("output".into())
    );
}

#[test]
fn command_output_is_literal_and_size_limited() {
    let doc = parse_strict("A=$(cmd)\n").unwrap();
    let resolved = doc
        .resolve_with_commands(
            |_| None,
            |_| Ok("$B".to_string()),
            None,
            &ExpandOptions {
                rules: crate::ReferenceRules::DOTENV_EXPAND,
                ..ExpandOptions::portable()
            },
        )
        .unwrap();
    assert_eq!(value(&resolved, 0), Some("$B"));

    let failure = doc
        .resolve_with_commands(
            |_| None,
            |_| Ok("too-large".to_string()),
            None,
            &ExpandOptions {
                rules: crate::ReferenceRules::DOTENV_EXPAND,
                max_value_bytes: 3,
                ..ExpandOptions::portable()
            },
        )
        .unwrap_err();
    assert!(matches!(failure.error, ResolveError::ValueTooLarge { .. }));
    assert_eq!(failure.commands.len(), 1);
}

#[test]
fn none_rules_leave_every_value_literal() {
    let doc = parse_strict("A=${B}\nB=x\n").unwrap();
    let resolved = doc.resolve(|_| None, &ExpandOptions::none()).unwrap();
    assert_eq!(value(&resolved, 0), Some("${B}"));
    assert_eq!(resolved.entries()[0].segments.len(), 1);
}

proptest! {
    #[test]
    fn resolved_segments_concatenate_and_origins_are_bounded(
        chars in prop::collection::vec(
            prop_oneof![
                Just('A'), Just('B'), Just('C'), Just('='), Just('$'), Just('{'),
                Just('}'), Just('('), Just(')'), Just(':'), Just('-'), Just('?'),
                Just('+'), Just('\\'), Just('"'), Just('\''), Just('`'), Just('\n'),
                Just('\r'), Just(' '), Just('#'), any::<char>(),
            ],
            0..240,
        )
    ) {
        let source: String = chars.into_iter().collect();
        let (document, _) = crate::parse(&source);
        for options in [ExpandOptions::portable(), ExpandOptions::compose()] {
            let Ok(resolved) = document.resolve(
                |key| Some(key.to_string()),
                &options,
            ) else { continue; };
            for entry in resolved.entries() {
                let Some(value) = &entry.value else {
                    prop_assert!(entry.segments.is_empty());
                    continue;
                };
                let joined: String = entry.segments.iter().map(|segment| segment.value.as_str()).collect();
                prop_assert_eq!(&joined, value);
                prop_assert!(value.len() <= options.max_value_bytes);
                for segment in &entry.segments {
                    if let ResolvedSegmentKind::Literal { span } = &segment.kind {
                        prop_assert!(source.get(span.clone()).is_some());
                    }
                }
            }
        }
    }
}
