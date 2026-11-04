use alloc::format;
#[cfg(feature = "serde")]
use alloc::string::ToString;

use crate::{mask, parse_strict, Comparison, ExpandOptions, FormatOptions};

#[test]
fn document_entry_item_and_edits_mask_values() {
    let doc = parse_strict("TOKEN=secret\nHOST=localhost\n").unwrap();
    let document = format!("{:?}", mask(&doc));
    assert!(document.contains("TOKEN"));
    assert!(!document.contains("secret"));
    assert!(document.contains("Document"));

    let entry = doc.get_entry("TOKEN").unwrap();
    let entry_debug = format!("{:?}", mask(entry));
    assert!(entry_debug.contains("TOKEN"));
    assert!(!entry_debug.contains("secret"));

    let item = doc.items().next().unwrap();
    let item_debug = format!("{:?}", mask(item));
    assert!(item_debug.contains("Item"), "{item_debug}");
    assert!(!item_debug.contains("secret"));

    let lookup = doc.lookup_index();
    let lookup_debug = format!("{:?}", mask(&lookup));
    assert!(lookup_debug.contains("LookupIndex"), "{lookup_debug}");
    assert!(!lookup_debug.contains("secret"));

    let effective = doc.effective_entries().next().unwrap();
    let effective_debug = format!("{:?}", mask(effective));
    assert!(
        effective_debug.contains("EffectiveEntry"),
        "{effective_debug}"
    );
    assert!(!effective_debug.contains("secret"));

    let index = doc.line_index();
    let index_debug = format!("{:?}", mask(&index));
    assert!(index_debug.contains("LineIndex"), "{index_debug}");
    assert!(!index_debug.contains("secret"));

    let group = doc.groups().next().unwrap();
    let group_debug = format!("{:?}", mask(group));
    assert!(group_debug.contains("Group"), "{group_debug}");
    assert!(!group_debug.contains("secret"));
    let all_groups_debug = format!("{:?}", mask(doc.groups()));
    assert!(all_groups_debug.contains("Groups"), "{all_groups_debug}");
    assert!(!all_groups_debug.contains("secret"));

    let edited = doc.set("TOKEN", "changed").unwrap();
    let edited_debug = format!("{:?}", mask(&edited));
    assert!(edited_debug.contains("VerifiedEdits"), "{edited_debug}");
    let first_edit = format!("{:?}", mask(&edited.edits()[0]));
    assert!(first_edit.contains("TextEdit"), "{first_edit}");
    assert!(!edited_debug.contains("changed"));
    assert!(!edited_debug.contains("secret"));
}

#[test]
fn format_and_comparison_mask_values() {
    let left = parse_strict("TOKEN=secret-left\n").unwrap();
    let right = parse_strict("TOKEN=secret-right\n").unwrap();
    let formatted = left.format(&FormatOptions::default()).unwrap();
    let formatted_debug = format!("{:?}", mask(&formatted));
    assert!(
        formatted_debug.contains("FormatResult"),
        "{formatted_debug}"
    );
    assert!(!formatted_debug.contains("left"));

    let comparison: Comparison = left.compare(&right);
    let comparison_debug = format!("{:?}", mask(&comparison));
    assert!(
        comparison_debug.contains("Comparison"),
        "{comparison_debug}"
    );
    assert!(comparison_debug.contains("TOKEN"));
    let compared = format!(
        "{:?}",
        mask(&comparison.differences()[0].left.clone().unwrap())
    );
    assert!(compared.contains("ComparedValue"), "{compared}");
    assert!(!comparison_debug.contains("secret-left"));
    assert!(!comparison_debug.contains("secret-right"));
}

#[test]
fn resolution_masks_values_and_command_output() {
    let doc = parse_strict("HOST=secret.example\nURL=https://${HOST}/\n").unwrap();
    let resolved = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
    let debug = format!("{:?}", mask(&resolved));
    assert!(debug.contains("ResolvedDocument"), "{debug}");
    assert!(debug.contains("HOST"));
    let entry = format!("{:?}", mask(&resolved.entries()[0]));
    assert!(entry.contains("ResolvedEntry"), "{entry}");
    let segment = format!("{:?}", mask(&resolved.entries()[0].segments[0]));
    assert!(segment.contains("ResolvedSegment"), "{segment}");
    assert!(!debug.contains("secret.example"));
    assert!(!debug.contains("https://"));
}

#[cfg(feature = "load")]
#[test]
fn loaded_environment_masks_values() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::ExpandScope;

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "adotenv-mask-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join(".env"), "TOKEN=secret\n").unwrap();
    let loaded = crate::load::load_cascade(&directory, "").unwrap();
    let debug = format!("{:?}", mask(&loaded));
    assert!(!debug.contains("secret"));
    assert!(debug.contains("LoadedEnvironment"));

    let values = loaded.clone().into_values();
    let values_debug = format!("{:?}", mask(&values));
    assert!(values_debug.contains("LoadedValues"), "{values_debug}");
    assert!(!values_debug.contains("secret"));

    let resolved = loaded
        .resolve_decoded(
            |_| None,
            &ExpandOptions::portable().with_scope(ExpandScope::WholeFile),
        )
        .unwrap();
    let resolved_debug = format!("{:?}", mask(&resolved));
    assert!(
        resolved_debug.contains("ResolvedEnvironment"),
        "{resolved_debug}"
    );
    assert!(!resolved_debug.contains("secret"));

    std::fs::write(directory.join(".env"), "A=${MISSING:?secret-required}\n").unwrap();
    let failed = crate::load::load_cascade(&directory, "").unwrap();
    let failure = failed
        .resolve_decoded(
            |_| None,
            &ExpandOptions::compose().with_scope(ExpandScope::WholeFile),
        )
        .unwrap_err();
    let failure_debug = format!("{:?}", mask(&failure));
    assert!(
        failure_debug.contains("LoadedResolveError"),
        "{failure_debug}"
    );
    assert!(!failure_debug.contains("secret-required"));
    std::fs::remove_dir_all(&directory).unwrap();
}

#[cfg(feature = "serde")]
#[test]
fn masked_deserialize_error_keeps_location_but_not_the_value() {
    #[allow(dead_code)]
    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    struct Config {
        port: u16,
    }

    let doc = parse_strict("PORT=secret-port\n").unwrap();
    let error = doc.deserialize::<Config>().unwrap_err();
    let masked = error.masked();
    assert_eq!(
        masked.to_string(),
        "line 1: <redacted> (key \"PORT\")",
        "{masked:?}"
    );
    assert!(format!("{masked:?}").contains("<redacted>"));
    assert!(!format!("{masked:?}").contains("secret-port"));
    let wrapper = format!("{:?}", crate::Masked::new(&error));
    assert!(wrapper.contains("<redacted>"), "{wrapper}");
    assert!(!wrapper.contains("secret-port"));
}

#[test]
fn command_and_failure_masks_keep_labels_and_hide_messages() {
    use alloc::string::ToString;

    use crate::{CommandCall, CommandError, CommandOutcome, ExpandOptions};

    let error = format!("{:?}", mask(CommandError::new("secret-command-error")));
    assert!(error.contains("CommandError"), "{error}");
    assert!(!error.contains("secret-command-error"));

    let outcome = format!(
        "{:?}",
        mask(CommandOutcome::Output("secret-command-output".to_string()))
    );
    assert!(outcome.contains("<redacted>"), "{outcome}");
    assert!(!outcome.contains("secret-command-output"));

    let call = CommandCall {
        reference: 0..1,
        payload: 0..1,
        source: Some("secret-source".to_string()),
        outcome: CommandOutcome::Error("secret-command-message".to_string()),
    };
    let call = format!("{:?}", mask(&call));
    assert!(call.contains("CommandCall"), "{call}");
    assert!(!call.contains("secret-command-message"));

    let doc = parse_strict("A=${MISSING:?secret-required-message}\n").unwrap();
    let failure = doc
        .resolve_with_commands(
            |_| None,
            |_| Err(CommandError::new("unused")),
            None,
            &ExpandOptions::compose(),
        )
        .unwrap_err();
    let masked_failure = format!("{:?}", mask(&failure));
    assert!(
        masked_failure.contains("ResolveFailure"),
        "{masked_failure}"
    );
    assert!(
        !masked_failure.contains("secret-required-message"),
        "{masked_failure}"
    );
    let masked_error = format!("{:?}", mask(&failure.error));
    assert!(masked_error.contains("ResolveError"), "{masked_error}");
    assert!(!masked_error.contains("secret-required-message"));
}
