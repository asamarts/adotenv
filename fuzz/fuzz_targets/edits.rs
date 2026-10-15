#![no_main]
#![forbid(unsafe_code)]

use adotenv::{parse, Document, Edit, InsertPosition, ItemKind, VerifiedEdits};
use libfuzzer_sys::fuzz_target;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Meaning {
    key: String,
    value: Option<String>,
    exported: bool,
}

fn meanings(document: &Document<'_>) -> Vec<Meaning> {
    document
        .entries()
        .map(|entry| Meaning {
            key: entry.key().to_owned(),
            value: entry.value().map(str::to_owned),
            exported: entry.exported(),
        })
        .collect()
}

fn insertion_anchor(
    document: &Document<'_>,
    key: &str,
    position: InsertPosition<'_>,
) -> (usize, usize) {
    match position {
        InsertPosition::End => (document.source().len(), document.entries().len()),
        InsertPosition::Sorted => document.entries().find(|entry| entry.key() > key).map_or(
            (document.source().len(), document.entries().len()),
            |entry| (entry.line_span().start, entry.id()),
        ),
        InsertPosition::AfterEntry(id) => {
            let entry = document
                .entry(id)
                .expect("successful insertion requires a valid anchor");
            (entry.line_span().end, id + 1)
        }
        InsertPosition::AfterKey(key) => {
            let entry = document
                .unique(key)
                .expect("successful insertion requires a unique anchor");
            (entry.line_span().end, entry.id() + 1)
        }
        _ => unreachable!("update the fuzz oracle for new insertion modes"),
    }
}

fn commented(raw: &str) -> String {
    let mut output = String::new();
    let mut pieces = raw.split_inclusive(['\r', '\n']).peekable();
    while let Some(piece) = pieces.next() {
        output.push_str("# ");
        output.push_str(piece);
        if piece.ends_with('\r') && pieces.peek() == Some(&"\n") {
            output.push_str(pieces.next().unwrap());
        }
    }
    output
}

// Derive permitted source targets from the requested operations, not from the
// producer's ranges. Exact whole-item/marker postconditions catch comment no-ops.
fn check_targets(document: &Document<'_>, operations: &[Edit<'_>], verified: &VerifiedEdits) {
    let mut allowed = Vec::new();
    let mut required = Vec::new();
    let mut selected = vec![false; document.items().len()];
    let mut eof_insertion = false;
    for operation in operations {
        match *operation {
            Edit::Set { entry, .. } => {
                let entry = document.entry(entry).unwrap();
                selected[entry.item_id()] = true;
                allowed.push(
                    entry
                        .value_span()
                        .unwrap_or(entry.key_span().end..entry.key_span().end),
                );
            }
            Edit::Rename { entry, .. } => {
                let entry = document.entry(entry).unwrap();
                selected[entry.item_id()] = true;
                allowed.push(entry.key_span());
            }
            Edit::Export { entry, enabled } => {
                let entry = document.entry(entry).unwrap();
                selected[entry.item_id()] = true;
                if enabled != entry.exported() {
                    allowed.push(if enabled {
                        entry.key_span().start..entry.key_span().start
                    } else {
                        entry.tokens().export.as_ref().unwrap().start
                            ..entry.tokens().space_after_export.end
                    });
                }
            }
            Edit::Remove { item } | Edit::Comment { item } | Edit::Uncomment { item } => {
                let item = document.item(item).unwrap();
                selected[item.id()] = true;
                let (range, replacement) = match operation {
                    Edit::Remove { .. } => (item.span(), String::new()),
                    Edit::Comment { .. } => (item.span(), commented(item.raw())),
                    _ => {
                        assert_eq!(item.kind(), ItemKind::Comment);
                        let hash = item.span().start + item.raw().find('#').unwrap();
                        let end = hash
                            + 1
                            + usize::from(
                                document.source().as_bytes().get(hash + 1) == Some(&b' '),
                            );
                        (hash..end, String::new())
                    }
                };
                allowed.push(range.clone());
                required.push((range, replacement));
            }
            Edit::Insert { key, position, .. } => {
                let (offset, _) = insertion_anchor(document, key, position);
                eof_insertion |= offset == document.source().len();
                allowed.push(offset..offset);
            }
            _ => unreachable!("update the fuzz oracle for new edit operations"),
        }
    }
    allowed.sort_unstable_by_key(|range| (range.start, range.end));
    allowed.dedup();
    for edit in verified.edits() {
        assert!(
            allowed
                .binary_search_by_key(&(edit.range.start, edit.range.end), |range| (
                    range.start,
                    range.end
                ))
                .is_ok(),
            "edit escaped its independently selected raw target"
        );
    }
    for (range, replacement) in required {
        let index = verified
            .edits()
            .binary_search_by_key(&(range.start, range.end), |edit| {
                (edit.range.start, edit.range.end)
            })
            .expect("selected raw operation was omitted");
        assert_eq!(verified.edits()[index].replacement, replacement);
    }

    let (after, _) = parse(verified.as_str());
    let mut actual = after.items();
    let mut changes = verified.edits().iter().peekable();
    let (mut removed, mut added) = (0, 0);
    for original in document.items().filter(|item| !selected[item.id()]) {
        while changes
            .peek()
            .is_some_and(|edit| edit.range.end <= original.span().start)
        {
            let edit = changes.next().unwrap();
            removed += edit.range.len();
            added += edit.replacement.len();
        }
        let start = original.span().start - removed + added;
        let retained = actual.find(|item| item.span().start >= start).unwrap();
        assert_eq!(retained.span().start, start);
        assert_eq!(retained.kind(), original.kind());
        let suffix = retained
            .raw()
            .strip_prefix(original.raw())
            .expect("unselected raw text changed");
        assert!(
            suffix.is_empty()
                || (eof_insertion
                    && original.span().end == document.source().len()
                    && original.line_ending() == adotenv::LineEnding::None
                    && matches!(suffix, "\n" | "\r" | "\r\n"))
        );
        let identity = |item: adotenv::Item<'_>| {
            item.diagnostic().map(|diagnostic| {
                (
                    diagnostic.code(),
                    diagnostic.severity(),
                    diagnostic.span().start - item.span().start
                        ..diagnostic.span().end - item.span().start,
                )
            })
        };
        assert_eq!(identity(retained), identity(original));
    }
}

fn check_replay(source: &str, verified: &VerifiedEdits) {
    let mut cursor = 0;
    let mut replay = String::new();
    for edit in verified.edits() {
        assert!(cursor <= edit.range.start && edit.range.start <= edit.range.end);
        assert!(source.get(edit.range.clone()).is_some());
        replay.push_str(&source[cursor..edit.range.start]);
        replay.push_str(&edit.replacement);
        cursor = edit.range.end;
    }
    replay.push_str(&source[cursor..]);
    assert_eq!(replay, verified.as_str());
}

fuzz_target!(|bytes: &[u8]| {
    // Header: operation byte, selector byte, UTF-8 source, NUL, UTF-8 value/key.
    let Some((&operation, rest)) = bytes.split_first() else {
        return;
    };
    let Some((&selector, payload)) = rest.split_first() else {
        return;
    };
    let split = payload
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(payload.len());
    let (source_bytes, tail) = payload.split_at(split);
    let value_bytes = tail.get(1..).unwrap_or_default();
    let (Ok(source), Ok(value)) = (
        std::str::from_utf8(source_bytes),
        std::str::from_utf8(value_bytes),
    ) else {
        return;
    };
    let (document, _) = parse(source);
    let before = meanings(&document);
    let mut expected = before.clone();
    let target = if selector & 0x40 != 0 {
        usize::from(selector)
    } else {
        usize::from(selector) % before.len().max(1)
    };
    let item_count = document.items().len();
    let item = usize::from(selector) % item_count.max(1);
    let key = if selector & 0x80 == 0 {
        "FUZZ_KEY"
    } else {
        value
    };
    let mut insertion = None;
    let mut uncommenting = false;
    let mut invalid_target = false;
    let mut operations = Vec::new();
    match operation % 11 {
        0 => {
            operations.push(Edit::Set {
                entry: target,
                value,
            });
            if let Some(entry) = expected.get_mut(target) {
                entry.value = Some(value.to_owned());
            } else {
                invalid_target = true;
            }
        }
        1 => {
            operations.push(Edit::Remove { item });
            if let Some(selected) = document.item(item) {
                if let Some(entry) = selected.entry() {
                    expected.remove(entry.id());
                }
            } else {
                invalid_target = true;
            }
        }
        2 => {
            operations.push(Edit::Rename { entry: target, key });
            if let Some(entry) = expected.get_mut(target) {
                entry.key = key.to_owned();
            } else {
                invalid_target = true;
            }
        }
        3 => {
            let position = match selector % 4 {
                0 => InsertPosition::End,
                1 => InsertPosition::Sorted,
                2 => InsertPosition::AfterEntry(target),
                _ => InsertPosition::AfterKey(
                    document
                        .entry(target)
                        .map_or("MISSING_FUZZ_KEY", |entry| entry.key()),
                ),
            };
            let value = (selector & 0x20 == 0).then_some(value);
            operations.push(Edit::Insert {
                key,
                value,
                position,
            });
            insertion = Some((
                Meaning {
                    key: key.to_owned(),
                    value: value.map(str::to_owned),
                    exported: false,
                },
                position,
            ));
        }
        4 => {
            operations.push(Edit::Comment { item });
            if let Some(selected) = document.item(item) {
                if let Some(entry) = selected.entry() {
                    expected.remove(entry.id());
                }
            } else {
                invalid_target = true;
            }
        }
        5 => {
            operations.push(Edit::Uncomment { item });
            uncommenting = true;
        }
        6 => {
            let enabled = selector & 0x20 == 0;
            operations.push(Edit::Export {
                entry: target,
                enabled,
            });
            if let Some(entry) = expected.get_mut(target) {
                entry.exported = enabled;
            } else {
                invalid_target = true;
            }
        }
        7 => {
            // One verified snapshot-local batch, not sequential reparsing.
            operations.push(Edit::Set {
                entry: target,
                value,
            });
            let other = target.saturating_add(1);
            operations.push(Edit::Rename { entry: other, key });
            if let Some(entry) = expected.get_mut(target) {
                entry.value = Some(value.to_owned());
            } else {
                invalid_target = true;
            }
            if let Some(entry) = expected.get_mut(other) {
                entry.key = key.to_owned();
            } else {
                invalid_target = true;
            }
        }
        8 | 9 => {
            // Remove an intervening selected comment as well as restoring lines.
            operations.extend(
                document
                    .items()
                    .filter(|item| item.kind() == ItemKind::Comment)
                    .map(|selected| {
                        if operation % 11 == 9 && selected.id() == item {
                            Edit::Remove {
                                item: selected.id(),
                            }
                        } else {
                            Edit::Uncomment {
                                item: selected.id(),
                            }
                        }
                    }),
            );
            uncommenting = true;
        }
        _ => {
            // Removing the old EOF item must preserve insertion ordering and the
            // original final-newline policy, including an empty surviving output.
            if let Some(last) = document.items().next_back() {
                operations.push(Edit::Remove { item: last.id() });
                if let Some(entry) = last.entry() {
                    expected.remove(entry.id());
                }
            }
            operations.push(Edit::Insert {
                key,
                value: Some(value),
                position: InsertPosition::End,
            });
            insertion = Some((
                Meaning {
                    key: key.to_owned(),
                    value: Some(value.to_owned()),
                    exported: false,
                },
                InsertPosition::End,
            ));
        }
    }
    let result = document.edit(&operations);
    if invalid_target {
        assert!(result.is_err());
    }
    let Ok(verified) = result else {
        return;
    };
    check_replay(source, &verified);
    check_targets(&document, &operations, &verified);
    let (after, _) = parse(verified.as_str());
    let actual = meanings(&after);
    if let Some((inserted, position)) = insertion {
        let (_, original_index) = insertion_anchor(&document, key, position);
        let removed_before = operations
            .iter()
            .filter(|operation| match operation {
                Edit::Remove { item } => document
                    .item(*item)
                    .and_then(|item| item.entry())
                    .is_some_and(|entry| entry.id() < original_index),
                _ => false,
            })
            .count();
        expected.insert(original_index - removed_before, inserted);
        assert_eq!(actual, expected);
    } else if uncommenting {
        // Previously interpreted entries must remain unchanged and in order.
        let mut remaining = actual.iter();
        for entry in &before {
            assert!(remaining.any(|candidate| candidate == entry));
        }
    } else {
        assert_eq!(actual, expected);
    }
});
