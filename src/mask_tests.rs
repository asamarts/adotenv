use alloc::format;

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
    assert!(!item_debug.contains("secret"));

    let index = doc.line_index();
    let index_debug = format!("{:?}", mask(&index));
    assert!(!index_debug.contains("secret"));

    let group = doc.groups().next().unwrap();
    let group_debug = format!("{:?}", mask(group));
    assert!(!group_debug.contains("secret"));
    let all_groups_debug = format!("{:?}", mask(doc.groups()));
    assert!(!all_groups_debug.contains("secret"));

    let edited = doc.set("TOKEN", "changed").unwrap();
    let edited_debug = format!("{:?}", mask(&edited));
    assert!(!edited_debug.contains("changed"));
    assert!(!edited_debug.contains("secret"));
}

#[test]
fn format_and_comparison_mask_values() {
    let left = parse_strict("TOKEN=secret-left\n").unwrap();
    let right = parse_strict("TOKEN=secret-right\n").unwrap();
    let formatted = left.format(&FormatOptions::default()).unwrap();
    let formatted_debug = format!("{:?}", mask(&formatted));
    assert!(!formatted_debug.contains("left"));

    let comparison: Comparison = left.compare(&right);
    let comparison_debug = format!("{:?}", mask(&comparison));
    assert!(comparison_debug.contains("TOKEN"));
    assert!(!comparison_debug.contains("secret-left"));
    assert!(!comparison_debug.contains("secret-right"));
}

#[test]
fn resolution_masks_values_and_command_output() {
    let doc = parse_strict("HOST=secret.example\nURL=https://${HOST}/\n").unwrap();
    let resolved = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
    let debug = format!("{:?}", mask(&resolved));
    assert!(debug.contains("HOST"));
    assert!(!debug.contains("secret.example"));
    assert!(!debug.contains("https://"));
}

#[cfg(feature = "load")]
#[test]
fn loaded_environment_masks_values() {
    use std::sync::atomic::{AtomicUsize, Ordering};

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
    assert!(!format!("{masked:?}").contains("secret-port"));
    assert!(!format!("{:?}", crate::Masked::new(&error)).contains("secret-port"));
}
