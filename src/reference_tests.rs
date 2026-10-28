use alloc::{string::String, vec, vec::Vec};
use core::ops::Range;
use proptest::prelude::*;

use crate::{
    parse, parse_with, Dialect, OperatorSet, Quote, QuotedTail, ReferenceKind, ReferenceOperator,
    ReferenceRules,
};

fn text<'a>(source: &'a str, span: &Range<usize>) -> &'a str {
    &source[span.clone()]
}

#[test]
fn short_and_braced_variables_have_exact_spans() {
    let source = "A=$VAR ${VAR} ${VAR:-default} ${VAR-default} ${VAR:?message} ${VAR?message} ${VAR:+alternative} ${VAR+alternative}\n";
    let (doc, _) = parse(source);
    let entry = doc.unique("A").unwrap();
    let references = entry.references();
    assert_eq!(references.len(), 8);
    for reference in &references {
        assert_eq!(reference.kind, ReferenceKind::Variable);
        assert_eq!(reference.quote, Quote::None);
        assert!(!reference.escaped);
        assert_eq!(reference.depth, 0);
        assert!(reference.closed);
        assert_eq!(text(source, reference.name.as_ref().unwrap()), "VAR");
    }
    assert_eq!(text(source, &references[0].span), "$VAR");
    assert!(!references[0].braced);
    assert_eq!(references[0].operator, None);
    assert_eq!(text(source, &references[1].span), "${VAR}");
    assert!(references[1].braced);
    assert_eq!(references[1].operator, None);

    let expected = [
        (ReferenceOperator::DefaultIfUnsetOrEmpty, ":-", "default"),
        (ReferenceOperator::DefaultIfUnset, "-", "default"),
        (ReferenceOperator::ErrorIfUnsetOrEmpty, ":?", "message"),
        (ReferenceOperator::ErrorIfUnset, "?", "message"),
        (
            ReferenceOperator::AlternativeIfSetAndNonEmpty,
            ":+",
            "alternative",
        ),
        (ReferenceOperator::AlternativeIfSet, "+", "alternative"),
    ];
    for (reference, (operator, operator_text, argument_text)) in
        references[2..].iter().zip(expected)
    {
        assert_eq!(reference.operator, Some(operator));
        assert_eq!(
            text(source, reference.operator_span.as_ref().unwrap()),
            operator_text
        );
        assert_eq!(
            text(source, reference.argument.as_ref().unwrap()),
            argument_text
        );
    }
}

#[test]
fn nested_defaults_are_reported_in_source_order() {
    let source = "A=${A:-${B:-deep}}\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert_eq!(text(source, &references[0].span), "${A:-${B:-deep}}");
    assert_eq!(references[0].depth, 0);
    assert_eq!(
        text(source, references[0].argument.as_ref().unwrap()),
        "${B:-deep}"
    );
    assert_eq!(text(source, &references[1].span), "${B:-deep}");
    assert_eq!(references[1].depth, 1);
    assert_eq!(text(source, references[1].name.as_ref().unwrap()), "B");
    assert_eq!(
        references[1].operator,
        Some(ReferenceOperator::DefaultIfUnsetOrEmpty)
    );
    assert!(references[0].is_eligible(ReferenceRules::COMPOSE));
    assert!(references[1].is_eligible(ReferenceRules::COMPOSE));
    assert!(!references[1].is_eligible(ReferenceRules::PORTABLE));
    assert_eq!(references[1].containers.len(), 1);
    assert_eq!(
        references[1].containers[0].operator,
        Some(ReferenceOperator::DefaultIfUnsetOrEmpty)
    );
    assert!(references[1].containers[0].name_nonempty);
    assert!(references[1].containers[0].closed);
}

#[test]
fn nested_candidates_require_eligible_containers() {
    let source = "A=$(echo $B)\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert_eq!(references[1].containers.len(), 1);
    assert_eq!(references[1].containers[0].kind, ReferenceKind::Command);
    assert!(!references[0].is_eligible(ReferenceRules::COMPOSE));
    assert!(!references[1].is_eligible(ReferenceRules::COMPOSE));
    assert!(references[0].is_eligible(ReferenceRules::DOTENV_EXPAND));
    assert!(references[1].is_eligible(ReferenceRules::DOTENV_EXPAND));

    let source = "A=${A:-$B}\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
    assert!(!references[1].is_eligible(ReferenceRules::PORTABLE));
    assert!(references[0].is_eligible(ReferenceRules::COMPOSE));
    assert!(references[1].is_eligible(ReferenceRules::COMPOSE));

    for source in ["A=${:-$B}\n", "A=${A:-$B\n"] {
        let (doc, _) = parse(source);
        let references = doc.unique("A").unwrap().references();
        assert!(references
            .iter()
            .all(|reference| { !reference.is_eligible(ReferenceRules::COMPOSE) }));
    }
}

#[test]
fn commands_and_nested_commands_keep_payload_spans() {
    let source = "A=$(echo $(date))\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert_eq!(text(source, &references[0].span), "$(echo $(date))");
    assert_eq!(references[0].kind, ReferenceKind::Command);
    assert_eq!(references[0].depth, 0);
    assert_eq!(
        text(source, references[0].command.as_ref().unwrap()),
        "echo $(date)"
    );
    assert_eq!(text(source, &references[1].span), "$(date)");
    assert_eq!(references[1].depth, 1);
    assert_eq!(
        text(source, references[1].command.as_ref().unwrap()),
        "date"
    );

    let quoted = "A=$(echo \")\")\n";
    let (doc, _) = parse(quoted);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 1);
    assert_eq!(text(quoted, &references[0].span), "$(echo \")\")");
    assert_eq!(
        text(quoted, references[0].command.as_ref().unwrap()),
        "echo \")\""
    );
    assert!(references[0].closed);
}

#[test]
fn nested_braces_and_commands_do_not_close_early() {
    let source = "A=${B:-$(echo })}\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert_eq!(text(source, &references[0].span), "${B:-$(echo })}");
    assert_eq!(
        text(source, references[0].argument.as_ref().unwrap()),
        "$(echo })"
    );
    assert_eq!(text(source, &references[1].span), "$(echo })");
    assert_eq!(
        text(source, references[1].command.as_ref().unwrap()),
        "echo }"
    );
    assert_eq!(references[1].depth, 1);

    let source = "A=$(echo ${B:-)})\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert_eq!(text(source, &references[0].span), "$(echo ${B:-)})");
    assert_eq!(
        text(source, references[0].command.as_ref().unwrap()),
        "echo ${B:-)}"
    );
    assert_eq!(text(source, &references[1].span), "${B:-)}");
    assert_eq!(text(source, references[1].argument.as_ref().unwrap()), ")");
    assert_eq!(references[1].depth, 1);
}

#[test]
fn escaped_literal_braces_do_not_close_the_outer_form() {
    let source = "A=${B:-\\${C}}\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert_eq!(text(source, &references[0].span), "${B:-\\${C}}");
    assert_eq!(
        text(source, references[0].argument.as_ref().unwrap()),
        "\\${C}"
    );
    assert_eq!(text(source, &references[1].span), "${C}");
    assert!(references[1].escaped);
    assert_eq!(references[1].depth, 1);
    assert!(!references[1].is_eligible(ReferenceRules::COMPOSE));
}

#[test]
fn malformed_forms_are_observable_but_ineligible() {
    for source in ["A=${VAR\n", "A=$(echo\n"] {
        let (doc, _) = parse(source);
        let references = doc.unique("A").unwrap().references();
        assert_eq!(references.len(), 1);
        assert!(!references[0].closed);
        assert!(!references[0].is_eligible(ReferenceRules::COMPOSE));
        assert!(!references[0].is_eligible(ReferenceRules::DOTENV_EXPAND));
    }
    let source = "A=${}\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 1);
    assert!(references[0].closed);
    assert!(references[0].name.as_ref().unwrap().is_empty());
    assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
}

#[test]
fn quote_and_escape_context_is_recorded() {
    let source = "A=\"\\$VAR $VAR\"\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert_eq!(references[0].quote, Quote::Double);
    assert!(references[0].escaped);
    assert!(!references[1].escaped);
    assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
    assert!(references[1].is_eligible(ReferenceRules::PORTABLE));

    let source = "A=\"\\\\$VAR\"\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 1);
    assert!(!references[0].escaped);
    assert!(references[0].is_eligible(ReferenceRules::PORTABLE));

    let source = "A='$VAR'\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references[0].quote, Quote::Single);
    assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
    let single = ReferenceRules {
        single_quotes: true,
        ..ReferenceRules::PORTABLE
    };
    assert!(references[0].is_eligible(single));

    let source = "A=`$VAR`\n";
    let (doc, _) = parse_with(source, Dialect::NODE_DOTENV);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references[0].quote, Quote::Backtick);
    assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
    let backtick = ReferenceRules {
        backtick_quotes: true,
        ..ReferenceRules::PORTABLE
    };
    assert!(references[0].is_eligible(backtick));

    let source = "A=$$VAR\n";
    let (doc, _) = parse_with(source, Dialect::COMPOSE);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 1);
    assert!(references[0].escaped);
    assert!(!references[0].is_eligible(ReferenceRules::COMPOSE));
    let escaped = ReferenceRules {
        escaped_dollar: true,
        ..ReferenceRules::COMPOSE
    };
    assert!(references[0].is_eligible(escaped));
}

#[test]
fn eligibility_applies_operator_and_quote_rules() {
    let source = "A=$VAR ${VAR} ${VAR:-x}\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 3);
    let braced = ReferenceRules {
        operators: OperatorSet::Braced,
        ..ReferenceRules::NONE
    };
    assert!(!references[0].is_eligible(braced));
    assert!(references[1].is_eligible(braced));
    assert!(!references[2].is_eligible(braced));

    let basic = ReferenceRules::PORTABLE;
    assert!(references[0].is_eligible(basic));
    assert!(references[1].is_eligible(basic));
    assert!(!references[2].is_eligible(basic));

    let compose = ReferenceRules::COMPOSE;
    for reference in &references {
        assert!(reference.is_eligible(compose));
    }
    for reference in &references {
        assert!(!reference.is_eligible(ReferenceRules::NONE));
    }

    let command = "A=$(echo hi)\n";
    let (doc, _) = parse(command);
    let references = doc.unique("A").unwrap().references();
    assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
    let with_commands = ReferenceRules {
        commands: true,
        ..ReferenceRules::PORTABLE
    };
    assert!(references[0].is_eligible(with_commands));
    assert!(references[0].is_eligible(ReferenceRules::DOTENV_EXPAND));
}

#[test]
fn appended_tail_references_use_unquoted_context() {
    let dialect = Dialect {
        quoted_tail: QuotedTail::Append,
        ..Dialect::PORTABLE
    };
    let source = "A=\"$BODY\"$TAIL\n";
    let (doc, _) = parse_with(source, dialect);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert_eq!(references[0].quote, Quote::Double);
    assert_eq!(text(source, &references[0].span), "$BODY");
    assert_eq!(references[1].quote, Quote::None);
    assert_eq!(text(source, &references[1].span), "$TAIL");
}

#[test]
fn bare_and_empty_entries_have_no_references() {
    let (doc, _) = parse("A\nB=\nC=x\n");
    assert_eq!(doc.unique("A").unwrap().references(), Vec::new());
    assert_eq!(doc.unique("B").unwrap().references(), Vec::new());
    assert_eq!(doc.unique("C").unwrap().references(), Vec::new());
}

proptest! {
    #[test]
    fn reference_spans_are_bounded_and_ordered(
        chars in prop::collection::vec(
            prop_oneof![
                Just('A'),
                Just('='),
                Just('"'),
                Just('\''),
                Just('`'),
                Just('\\'),
                Just('$'),
                Just('{'),
                Just('}'),
                Just('('),
                Just(')'),
                Just(':'),
                Just('-'),
                Just('?'),
                Just('+'),
                Just('\n'),
                Just('\r'),
                Just(' '),
                Just('#'),
                any::<char>(),
            ],
            0..300,
        )
    ) {
        let source: String = chars.into_iter().collect();
        let (doc, _) = parse(&source);
        for entry in doc.entries() {
            let references = entry.references();
            let mut previous_start = 0;
            for reference in &references {
                prop_assert!(reference.span.start >= previous_start);
                previous_start = reference.span.start;
                prop_assert!(reference.span.start < reference.span.end);
                prop_assert!(source.get(reference.span.clone()).is_some());
                prop_assert!(source.is_char_boundary(reference.span.start));
                prop_assert!(source.is_char_boundary(reference.span.end));
                if let Some(value_span) = entry.value_span() {
                    prop_assert!(reference.span.start >= value_span.start);
                    prop_assert!(reference.span.end <= value_span.end);
                }
                for span in [
                    reference.name.as_ref(),
                    reference.operator_span.as_ref(),
                    reference.argument.as_ref(),
                    reference.command.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    prop_assert!(span.start >= reference.span.start);
                    prop_assert!(span.end <= reference.span.end);
                    prop_assert!(source.get(span.clone()).is_some());
                    prop_assert!(source.is_char_boundary(span.start));
                    prop_assert!(source.is_char_boundary(span.end));
                }
                prop_assert_eq!(reference.operator.is_some(), reference.operator_span.is_some());
                prop_assert_eq!(reference.operator.is_some(), reference.argument.is_some());
                prop_assert_eq!(
                    reference.kind == ReferenceKind::Command,
                    reference.command.is_some()
                );
                prop_assert_eq!(
                    reference.kind == ReferenceKind::Variable,
                    reference.name.is_some()
                );
                let _ = reference.is_eligible(ReferenceRules::PORTABLE);
                let _ = reference.is_eligible(ReferenceRules::COMPOSE);
                let _ = reference.is_eligible(ReferenceRules::DOTENV_EXPAND);
                let _ = reference.is_eligible(ReferenceRules::NONE);
            }
        }
    }
}

#[test]
fn reference_rule_builders_and_defaults_are_complete() {
    let defaults = ReferenceRules::default();
    assert_eq!(defaults, ReferenceRules::PORTABLE);
    let rules = defaults
        .with_operators(OperatorSet::Compose)
        .with_single_quotes(true)
        .with_escaped_dollar(true)
        .with_commands(true)
        .with_backtick_quotes(true);
    assert_eq!(rules.operators, OperatorSet::Compose);
    assert!(rules.single_quotes);
    assert!(rules.escaped_dollar);
    assert!(rules.commands);
    assert!(rules.backtick_quotes);
}

#[test]
fn escaped_and_unclosed_containers_control_nested_eligibility() {
    let source = "A=\"${B:-\\$C}\"\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    let nested = &references[1];
    assert!(nested.escaped);
    assert_eq!(nested.depth, 1);
    assert!(!nested.is_eligible(ReferenceRules::COMPOSE));
    let escaped = ReferenceRules {
        escaped_dollar: true,
        ..ReferenceRules::COMPOSE
    };
    assert!(nested.is_eligible(escaped));

    let source = "A=${B:-$C\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert!(!references[0].closed);
    assert!(!references[1].is_eligible(ReferenceRules::COMPOSE));

    let source = "A=${:-$B}\n";
    let (doc, _) = parse(source);
    let references = doc.unique("A").unwrap().references();
    assert_eq!(references.len(), 2);
    assert!(!references[1].is_eligible(ReferenceRules::COMPOSE));
}
