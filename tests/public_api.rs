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

#[test]
fn public_1_6_api_end_to_end_flow() {
    use adotenv::{
        mask, parse_entries_compact, parse_entries_compact_with,
        parse_entries_compact_with_options, parse_strict_with, parse_strict_with_options,
        AssignmentState, BomPolicy, CompactEntries, Dialect, EditError, EditOptions, LimitKind,
        ParseOptions,
    };

    // BomPolicy::StripAll + Document::bom_span
    let strip_all = Dialect::PORTABLE.with_bom(BomPolicy::StripAll);
    let doc = parse_strict_with("\u{feff}\u{feff}A=1\nB=2\nBARE\n", strip_all).unwrap();
    assert!(doc.has_bom());
    assert_eq!(doc.bom_span(), Some(0..6));

    // LookupIndex::unique matches Document::unique
    let index = doc.lookup_index();
    assert_eq!(index.unique("A").unwrap().value(), Some("1"));
    assert_eq!(index.unique("BARE").unwrap().state(), AssignmentState::Bare);
    assert_eq!(index.unique("MISSING").unwrap_err(), EditError::Missing);

    // ParseOptions::with_max_lines
    let line_err = parse_strict_with_options(
        "A=1\nB=2\nC=3\n",
        Dialect::PORTABLE,
        ParseOptions::new().with_max_lines(2),
    )
    .unwrap_err();
    assert_eq!(line_err.limit().unwrap().kind(), LimitKind::Lines);

    // EditOptions across *_with_options helpers
    let opts = EditOptions::new()
        .with_max_output_bytes(64)
        .with_max_output_lines(5)
        .with_parse_limits(ParseOptions::new().with_max_lines(5));
    let edited = doc.set_with_options("A", "updated", opts).unwrap();
    assert!(edited.as_str().contains("A=updated\n"));
    let removed = doc.remove_with_options("B", opts).unwrap();
    assert!(!removed.as_str().contains("B=2"));
    let renamed = doc.rename_with_options("B", "B_NEW", opts).unwrap();
    assert!(renamed.as_str().contains("B_NEW=2"));
    let ref_doc = parse_strict("HOST=localhost\nURL=$HOST/api\n").unwrap();
    let ref_renamed = ref_doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER",
            EditOptions::new()
                .with_max_output_bytes(64)
                .with_max_output_lines(3)
                .with_parse_limits(ParseOptions::new().with_max_lines(3)),
        )
        .unwrap();
    assert_eq!(ref_renamed.as_str(), "SERVER=localhost\nURL=$SERVER/api\n");
    let ref_too_big = ref_doc
        .rename_with_references_with_options(
            "HOST",
            "VERY_LONG_SERVER_NAME",
            EditOptions::new().with_max_output_bytes(24),
        )
        .unwrap_err();
    assert!(matches!(
        ref_too_big,
        EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes
    ));
    let inserted = doc
        .insert_with_options("C", Some("3"), InsertPosition::End, opts)
        .unwrap();
    assert!(inserted.as_str().contains("C=3"));
    let too_big = doc
        .set_with_options(
            "A",
            "1234567890",
            EditOptions::new().with_max_output_bytes(8),
        )
        .unwrap_err();
    assert!(matches!(
        too_big,
        EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes
    ));

    // CompactEntries + Mask
    let entries: CompactEntries = parse_entries_compact("A=secret\nBARE\nEMPTY=\n").unwrap();
    assert_eq!(entries.get("A"), Some(Some("secret")));
    assert_eq!(entries.get("BARE"), Some(None));
    assert_eq!(entries.get("EMPTY"), Some(Some("")));
    assert_eq!(entries.get_state("BARE"), Some(AssignmentState::Bare));
    let masked = format!("{:?}", mask(&entries));
    assert!(masked.contains("CompactEntries"));
    assert!(masked.contains("len: 3"));
    assert!(!masked.contains("secret"));

    let entries_with = parse_entries_compact_with("\u{feff}\u{feff}A=1\n", strip_all).unwrap();
    assert_eq!(entries_with.get_value("A"), Some("1"));
    let entries_limited = parse_entries_compact_with_options(
        "A=1\nB=2\n",
        Dialect::PORTABLE,
        ParseOptions::new().with_max_lines(1),
    )
    .unwrap_err();
    assert_eq!(entries_limited.limit().unwrap().kind(), LimitKind::Lines);
}

#[test]
fn public_1_8_api_end_to_end_flow() {
    use adotenv::{
        parse_strict, BomPolicy, ColumnEncoding, EditError, EditOptions, EqualsSpacing,
        FormatOptions, LimitKind, LineIndex, ParseOptions, Position,
    };

    // Bounded edit, format, and sort helpers + LimitKind::References / max_references.
    let doc = parse_strict("B=2\nA=1\nC=$A\n").unwrap();
    let opts = EditOptions::new()
        .with_max_output_bytes(64)
        .with_max_output_lines(4)
        .with_max_references(4)
        .with_parse_limits(ParseOptions::new().with_max_lines(4));

    let commented = doc.comment_with_options("B", opts).unwrap();
    assert_eq!(commented.as_str(), "# B=2\nA=1\nC=$A\n");
    let uncommented = parse_strict(commented.as_str())
        .unwrap()
        .uncomment_with_options(0, opts)
        .unwrap();
    assert_eq!(uncommented.as_str(), "B=2\nA=1\nC=$A\n");

    let exported = doc.set_export_with_options("A", true, opts).unwrap();
    assert_eq!(exported.as_str(), "B=2\nexport A=1\nC=$A\n");

    let formatted = doc
        .format_with_options(
            &FormatOptions::default().with_equals_spacing(EqualsSpacing::Spaces),
            opts,
        )
        .unwrap();
    assert_eq!(formatted.as_str(), "B = 2\nA = 1\nC = $A\n");
    let fmt_err = doc
        .format_with_options(
            &FormatOptions::default().with_equals_spacing(EqualsSpacing::Spaces),
            EditOptions::new().with_max_output_bytes(6),
        )
        .unwrap_err();
    assert!(matches!(
        fmt_err,
        EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes
    ));

    let sorted = doc.sort_with_options(opts).unwrap();
    assert_eq!(sorted.as_str(), "A=1\nB=2\nC=$A\n");
    let sort_ref_err = doc
        .sort_with_options(EditOptions::new().with_max_references(0))
        .unwrap_err();
    assert!(matches!(
        sort_ref_err,
        EditError::Limit(limit) if limit.kind() == LimitKind::References
    ));

    let rename_ref_err = doc
        .rename_with_references_with_options("A", "D", EditOptions::new().with_max_references(0))
        .unwrap_err();
    assert!(matches!(
        rename_ref_err,
        EditError::Limit(limit) if limit.kind() == LimitKind::References
    ));

    // LineIndex::with_bom_policy across all four BomPolicy variants.
    let multi_bom = "\u{feff}\u{feff}A=1\n";
    let strip = LineIndex::new(multi_bom).with_bom_policy(BomPolicy::Strip);
    let strip_all = LineIndex::new(multi_bom).with_bom_policy(BomPolicy::StripAll);
    let keep = LineIndex::new(multi_bom).with_bom_policy(BomPolicy::Keep);
    let err_policy = LineIndex::new(multi_bom).with_bom_policy(BomPolicy::Error);
    assert_eq!(
        strip.to_position(6, ColumnEncoding::Scalar).unwrap(),
        Position { line: 0, column: 1 }
    );
    assert_eq!(
        strip_all.to_position(6, ColumnEncoding::Scalar).unwrap(),
        Position { line: 0, column: 0 }
    );
    assert_eq!(
        keep.to_position(6, ColumnEncoding::Scalar).unwrap(),
        Position { line: 0, column: 2 }
    );
    assert_eq!(
        err_policy.to_position(6, ColumnEncoding::Scalar).unwrap(),
        Position { line: 0, column: 2 }
    );
}

#[cfg(feature = "load")]
#[test]
fn public_1_8_compact_entry_disk_loaders_end_to_end() {
    use std::{env, fs, time::SystemTime};

    use adotenv::{
        load::{
            load_cascade_entries_compact, load_cascade_entries_compact_with,
            load_cascade_entries_compact_with_options, load_entries_compact,
            load_entries_compact_with_options, LoadOptions,
        },
        AssignmentState, Dialect,
    };

    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = env::temp_dir().join(format!("adotenv-public-1-8-{unique}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".env"), "PORT=3000\nBASE_ONLY=yes\n").unwrap();
    fs::write(dir.join(".env.local"), "PORT=8080\nPORT\nBARE_LOCAL\n").unwrap();

    let single = load_entries_compact([dir.join(".env.local")], Dialect::PORTABLE).unwrap();
    assert_eq!(single.get("PORT"), Some(None));
    assert_eq!(single.get_state("BARE_LOCAL"), Some(AssignmentState::Bare));

    let single_opts =
        load_entries_compact_with_options([dir.join(".env")], LoadOptions::new(Dialect::PORTABLE))
            .unwrap();
    assert_eq!(single_opts.get_value("PORT"), Some("3000"));

    // Cascade merge preserves earlier assigned value when a later file declares a bare key.
    let cascade = load_cascade_entries_compact(&dir, "").unwrap();
    assert_eq!(cascade.get("PORT"), Some(Some("3000")));
    assert_eq!(cascade.get("BASE_ONLY"), Some(Some("yes")));
    assert_eq!(cascade.get("BARE_LOCAL"), Some(None));

    let cascade_with = load_cascade_entries_compact_with(&dir, "", Dialect::PORTABLE).unwrap();
    assert_eq!(cascade_with.get("PORT"), Some(Some("3000")));

    let cascade_opts =
        load_cascade_entries_compact_with_options(&dir, "", LoadOptions::new(Dialect::PORTABLE))
            .unwrap();
    assert_eq!(cascade_opts.get("PORT"), Some(Some("3000")));

    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(feature = "serde")]
#[test]
fn public_1_8_serde_non_string_scalar_map_keys() {
    use std::collections::BTreeMap;

    use adotenv::{parse_strict_with, Dialect};

    let doc = parse_strict_with("80=http\n443=https\n", Dialect::NODE_DOTENV).unwrap();
    let ports: BTreeMap<u16, String> = doc.deserialize().unwrap();
    assert_eq!(ports.get(&80).map(String::as_str), Some("http"));
    assert_eq!(ports.get(&443).map(String::as_str), Some("https"));
}
