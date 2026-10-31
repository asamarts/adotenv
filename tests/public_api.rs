//! Public API integration tests.

use adotenv::{parse_strict, parse_values, Edit, InsertPosition};

#[test]
fn public_values_parsing_and_verified_edit_flow() {
    let values = parse_values("A=one\nA=two\nB=three\n").unwrap();
    assert_eq!(values.get("A").map(String::as_str), Some("two"));

    let doc = parse_strict("A=one\nB=two\n").unwrap();
    let edited = doc
        .edit(&[
            Edit::Set {
                entry: doc.unique("A").unwrap().id(),
                value: "changed",
            },
            Edit::Insert {
                key: "C",
                value: Some("three"),
                position: InsertPosition::End,
            },
        ])
        .unwrap();
    let reparsed = parse_strict(edited.as_str()).unwrap();
    assert_eq!(reparsed.get("A"), Some("changed"));
    assert_eq!(reparsed.get("C"), Some("three"));
}

#[test]
fn setting_and_inserting_after_every_entry_stays_linear() {
    use std::{
        fmt::Write as _,
        time::{Duration, Instant},
    };

    const COUNT: usize = 40_000;
    let mut source = String::with_capacity(COUNT * 10);
    for index in 0..COUNT {
        writeln!(source, "K{index:06}=x").unwrap();
    }
    let doc = parse_strict(&source).unwrap();
    let keys: Vec<String> = (0..COUNT).map(|index| format!("N{index:06}")).collect();
    let mut operations = Vec::with_capacity(COUNT * 2);
    for (entry, key) in keys.iter().enumerate() {
        operations.push(Edit::Set { entry, value: "y" });
        operations.push(Edit::Insert {
            key,
            value: Some("z"),
            position: InsertPosition::AfterEntry(entry),
        });
    }
    let started = Instant::now();
    let edited = doc.edit(&operations).unwrap();
    let elapsed = started.elapsed();
    // A per-operation scan of the whole source made this batch quadratic:
    // more than 20 seconds even in an optimized build. Linear planning takes
    // a small fraction of the bound in an unoptimized build.
    assert!(
        elapsed < Duration::from_secs(20),
        "{COUNT}-entry set and insert batch took {elapsed:?}"
    );
    let after = parse_strict(edited.as_str()).unwrap();
    assert_eq!(after.entries().len(), COUNT * 2);
    for (index, entry) in after.entries().enumerate() {
        let (prefix, value) = if index % 2 == 0 {
            ('K', "y")
        } else {
            ('N', "z")
        };
        assert_eq!(entry.key(), format!("{prefix}{:06}", index / 2));
        assert_eq!(entry.value(), Some(value));
    }
}

#[cfg(feature = "load")]
#[test]
fn public_loading_and_origin_flow() {
    use std::fs;

    use adotenv::load::{
        load_cascade, load_cascade_values, load_cascade_with_options, LoadOptions,
    };

    let dir = std::env::temp_dir().join(format!(
        "adotenv-public-api-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".env"), "A=base\n").unwrap();
    fs::write(dir.join(".env.local"), "A=local\nB=two\n").unwrap();

    let loaded = load_cascade(&dir, "").unwrap();
    assert_eq!(loaded.get("A"), Some("local"));
    assert_eq!(loaded.origin("A").unwrap().path(), dir.join(".env.local"));
    assert_eq!(loaded.entries().count(), 0);

    let full =
        load_cascade_with_options(&dir, "", LoadOptions::full(adotenv::Dialect::PORTABLE)).unwrap();
    assert_eq!(full.entries().count(), 3);
    assert_eq!(full.overrides().count(), 1);

    let values = load_cascade_values(&dir, "").unwrap();
    assert_eq!(values.get("A"), Some("local"));

    fs::remove_dir_all(&dir).unwrap();
}
