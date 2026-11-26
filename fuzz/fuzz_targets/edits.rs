#![no_main]
#![forbid(unsafe_code)]

use std::ops::Range;

use adotenv::{
    parse, parse_with, Dialect, Document, Edit, EditError, EditOptions, InsertPosition, ItemKind,
    LimitKind, VerifiedEdits,
};
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

fn map_offset(
    offset: usize,
    verified: &VerifiedEdits,
    include_insertions_at_boundary: bool,
) -> usize {
    verified.edits().iter().fold(offset, |mapped, edit| {
        let before = edit.range.end < offset
            || (edit.range.end == offset
                && (!edit.range.is_empty() || include_insertions_at_boundary));
        if before {
            mapped - edit.range.len() + edit.replacement.len()
        } else {
            mapped
        }
    })
}

fn mapped_range(range: Range<usize>, verified: &VerifiedEdits) -> Range<usize> {
    map_offset(range.start, verified, false)..map_offset(range.end, verified, true)
}

fn mapped_range_after(range: Range<usize>, verified: &VerifiedEdits) -> Range<usize> {
    map_offset(range.start, verified, true)..map_offset(range.end, verified, true)
}

fn insertion_anchor(
    document: &Document<'_>,
    key: &str,
    position: InsertPosition<'_>,
) -> Option<(usize, usize)> {
    Some(match position {
        InsertPosition::End => (document.source().len(), document.entries().len()),
        InsertPosition::Sorted => document.entries().find(|entry| entry.key() > key).map_or(
            (document.source().len(), document.entries().len()),
            |entry| (entry.line_span().start, entry.id()),
        ),
        InsertPosition::AfterEntry(id) => {
            let entry = document.entry(id)?;
            (entry.line_span().end, id + 1)
        }
        InsertPosition::AfterKey(key) => {
            let entry = document.unique(key).ok()?;
            (entry.line_span().end, entry.id() + 1)
        }
        _ => unreachable!("update the fuzz oracle for new insertion modes"),
    })
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

// Whether a declaration of `key` survives the batch. Insertions and renames
// require a key that the output does not otherwise declare; removal frees a key.
fn key_survives(document: &Document<'_>, operations: &[Edit<'_>], key: &str) -> bool {
    document.get_all(key).any(|entry| {
        !operations
            .iter()
            .any(|operation| matches!(*operation, Edit::Remove { item } if item == entry.item_id()))
    })
}

fn valid_value(value: &str) -> bool {
    value
        .chars()
        .all(|ch| !ch.is_control() || matches!(ch, '\r' | '\n' | '\t'))
}

// Positive cases only: uncertain contexts may be rejected, but ordinary valid
// edits must not silently turn into blanket refusal. The generated batches use
// distinct properties/targets except the intentional remove-plus-EOF insertion.
fn known_safe(document: &Document<'_>, operations: &[Edit<'_>]) -> bool {
    if document
        .items()
        .any(|item| item.kind() == ItemKind::Invalid)
    {
        return false;
    }
    operations.iter().all(|operation| match *operation {
        Edit::Set { entry, value } => document.entry(entry).is_some() && valid_value(value),
        Edit::Rename { entry, key } => document.entry(entry).is_some_and(|entry| {
            valid_key(key)
                && key != "export"
                && (key == entry.key() || document.get_entry(key).is_none())
        }),
        Edit::Export { entry, enabled } => document
            .entry(entry)
            .is_some_and(|entry| enabled || !entry.exported() || entry.key() != "export"),
        Edit::Comment { item } => document.item(item).is_some(),
        Edit::Remove { item } => document.item(item).is_some_and(|item| {
            let span = item.span();
            !(document.source()[..span.start].ends_with('\r')
                && document.source()[span.end..].starts_with('\n'))
        }),
        Edit::Insert {
            key,
            value,
            position,
        } => {
            valid_key(key)
                && value.is_none_or(valid_value)
                && !key_survives(document, operations, key)
                && insertion_anchor(document, key, position).is_some_and(|(offset, _)| {
                    document.source().as_bytes().get(offset) != Some(&b'\n')
                })
        }
        Edit::Uncomment { item } => {
            document.items().len() == 1
                && document.item(item).is_some_and(|item| {
                    if item.kind() != ItemKind::Comment {
                        return false;
                    }
                    let raw = item.raw();
                    let indent = raw
                        .bytes()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count();
                    assert_eq!(raw.as_bytes().get(indent), Some(&b'#'));
                    let mut body = indent + 1;
                    if raw.as_bytes().get(body) == Some(&b' ') {
                        body += 1;
                    }
                    let mut restored = document.source()[..item.span().start].to_owned();
                    restored.push_str(&raw[..indent]);
                    restored.push_str(&raw[body..]);
                    let (after, _) = parse(&restored);
                    after.bom_span() == document.bom_span()
                        && after.items().all(|item| item.kind() != ItemKind::Invalid)
                })
        }
        _ => unreachable!("update the positive oracle for new edit operations"),
    })
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
                let (offset, _) = insertion_anchor(document, key, position).unwrap();
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
    // The documented contract: apply the slice in reverse order.
    let mut reversed = source.to_owned();
    for edit in verified.edits().iter().rev() {
        reversed.replace_range(edit.range.clone(), &edit.replacement);
    }
    assert_eq!(reversed, verified.as_str());
}

fn check_comment_restore(source: &str, selector: u8, dialect: Dialect) {
    let (document, _) = parse_with(source, dialect);
    let Some(entry) = document.entry(usize::from(selector) % document.entries().len().max(1))
    else {
        return;
    };
    let item = document.item(entry.item_id()).unwrap();
    let result = document.edit(&[Edit::Comment { item: item.id() }]);
    let fits = dialect
        .line_byte_limit
        .is_none_or(|limit| item.raw().trim_end_matches('\n').len().saturating_add(2) <= limit);
    if dialect == Dialect::DOCKER_RAW && fits {
        assert!(
            result.is_ok(),
            "ordinary Docker comment refused: {result:?}"
        );
    }
    let Ok(comment) = result else { return };
    check_replay(source, &comment);
    let body = if dialect.line_breaks == adotenv::LineBreaks::Lf {
        item.raw()
            .split_inclusive('\n')
            .map(|line| format!("# {line}"))
            .collect::<String>()
    } else {
        commented(item.raw())
    };
    let mut expected = source.to_owned();
    expected.replace_range(item.span(), &body);
    assert_eq!(comment.as_str(), expected);
    if item
        .diagnostic()
        .is_some_and(|d| d.severity() == adotenv::Severity::Error)
    {
        return;
    }
    let (comments, _) = parse_with(comment.as_str(), dialect);
    let end = item.span().start + body.len();
    let operations = comments
        .items()
        .filter(|part| part.span().start >= item.span().start && part.span().end <= end)
        .map(|part| Edit::Uncomment { item: part.id() })
        .collect::<Vec<_>>();
    let restored = comments.edit(&operations);
    if document
        .entries()
        .filter(|other| other.key() == entry.key())
        .count()
        > 1
    {
        // Restoring a declaration of a key that the file still declares
        // collides, as an insertion would.
        assert!(
            matches!(restored, Err(adotenv::EditError::Duplicate(_))),
            "{restored:?}"
        );
        return;
    }
    let restored = restored.expect("generated comments must restore valid syntax");
    check_replay(comment.as_str(), &restored);
    assert_eq!(restored.as_str(), source);
}

fn check_selected(source: &str, value: &str, operation: u8, selector: u8, dialect: Dialect) {
    let (document, _) = parse_with(source, dialect);
    let count = document.entries().len();
    if count == 0 {
        return;
    }
    let target = usize::from(selector) % count;
    let entry = document.entry(target).unwrap();
    let item_id = entry.item_id();
    let records = |doc: &Document<'_>, omit: Option<usize>, selected: Option<&Range<usize>>| {
        doc.items()
            .filter(|item| {
                Some(item.id()) != omit
                    && !selected.is_some_and(|range| {
                        item.span().start >= range.start
                            && item.span().end <= range.end
                            && matches!(item.kind(), ItemKind::Blank | ItemKind::Comment)
                    })
            })
            .map(|item| {
                let diagnostic = item.diagnostic().map(|d| {
                    let span = d.span();
                    (
                        d.code(),
                        span.start - item.span().start,
                        span.end - item.span().start,
                    )
                });
                (item.kind(), item.raw().to_owned(), diagnostic)
            })
            .collect::<Vec<_>>()
    };
    let mut expected = meanings(&document);
    let (edit, allowed) = match operation % 4 {
        0 => {
            expected[target].value = Some(value.to_owned());
            let range = entry
                .value_span()
                .unwrap_or(entry.tokens().key.end..entry.tokens().key.end);
            (
                Edit::Set {
                    entry: target,
                    value,
                },
                range,
            )
        }
        1 => {
            expected.remove(target);
            (Edit::Remove { item: item_id }, entry.line_span())
        }
        2 => {
            expected[target].key = value.to_owned();
            (
                Edit::Rename {
                    entry: target,
                    key: value,
                },
                entry.tokens().key.clone(),
            )
        }
        _ => {
            expected[target].exported = !entry.exported();
            let range = if let Some(export) = &entry.tokens().export {
                export.start..entry.tokens().space_after_export.end
            } else {
                entry.tokens().key.start..entry.tokens().key.start
            };
            (
                Edit::Export {
                    entry: target,
                    enabled: !entry.exported(),
                },
                range,
            )
        }
    };
    let Ok(verified) = document.edit(&[edit]) else {
        return;
    };
    check_replay(source, &verified);
    for replacement in verified.edits() {
        assert!(allowed.start <= replacement.range.start && replacement.range.end <= allowed.end);
    }
    let (after, _) = parse_with(verified.as_str(), dialect);
    assert_eq!(meanings(&after), expected);
    let before_records = records(&document, Some(item_id), None);
    let after_item = if operation % 4 == 1 {
        None
    } else {
        Some(after.entry(target).unwrap().item_id())
    };
    let selected = mapped_range(document.item(item_id).unwrap().span(), &verified);
    assert_eq!(records(&after, after_item, Some(&selected)), before_records);
}

fn check_rename_with_references(source: &str, value: &str, selector: u8, dialect: Dialect) {
    let (document, _) = parse_with(source, dialect);
    let Some(entry) = document.entry(usize::from(selector) % document.entries().len().max(1))
    else {
        return;
    };
    let ref_key = if selector & 0x80 == 0 {
        "FUZZ_REF_KEY"
    } else {
        value
    };
    let Ok(renamed) = document.rename_with_references(entry.key(), ref_key) else {
        return;
    };
    check_replay(source, &renamed);
    let exact_bytes = document.rename_with_references_with_options(
        entry.key(),
        ref_key,
        EditOptions::new().with_max_output_bytes(renamed.as_str().len()),
    );
    assert_eq!(exact_bytes, Ok(renamed.clone()));
    if !renamed.as_str().is_empty() {
        let too_small = document.rename_with_references_with_options(
            entry.key(),
            ref_key,
            EditOptions::new().with_max_output_bytes(renamed.as_str().len() - 1),
        );
        assert!(matches!(
            too_small,
            Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes
        ));
    }
    let (after, _) = parse_with(renamed.as_str(), dialect);
    assert_eq!(after.bom_span(), document.bom_span());
    assert_eq!(after.items().len(), document.items().len());
    let mut expected_meanings = meanings(&document);
    expected_meanings[entry.id()].key = ref_key.to_owned();
    let actual_meanings = meanings(&after);
    assert_eq!(actual_meanings.len(), expected_meanings.len());
    for ((before_entry, after_entry), expected) in document
        .entries()
        .zip(after.entries())
        .zip(expected_meanings.iter())
    {
        assert_eq!(after_entry.key(), expected.key);
        assert_eq!(after_entry.exported(), expected.exported);
        let before_refs = before_entry.references();
        let after_refs = after_entry.references();
        assert_eq!(before_refs.len(), after_refs.len());
        let mut had_matching_ref = false;
        for (br, ar) in before_refs.iter().zip(after_refs.iter()) {
            let orig_name = br.name.as_ref().and_then(|s| source.get(s.clone()));
            let new_name = ar
                .name
                .as_ref()
                .and_then(|s| renamed.as_str().get(s.clone()));
            let was_renamed = orig_name == Some(entry.key());
            had_matching_ref |= was_renamed;
            let mut ancestor = br.parent;
            let mut in_command = false;
            while let Some(idx) = ancestor {
                if before_refs[idx].kind == adotenv::ReferenceKind::Command {
                    in_command = true;
                    break;
                }
                ancestor = before_refs[idx].parent;
            }
            if !in_command {
                in_command = before_refs.iter().any(|candidate| {
                    candidate.kind == adotenv::ReferenceKind::Command
                        && candidate.span.start < br.span.start
                });
            }
            if was_renamed && entry.key() != ref_key && in_command {
                let is_safe_cmd_key = |key: &str| {
                    let mut bytes = key.bytes();
                    bytes
                        .next()
                        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
                        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
                        && !key.ends_with('.')
                        && !key.contains("..")
                };
                assert!(
                    is_safe_cmd_key(ref_key) && is_safe_cmd_key(entry.key()),
                    "unsafe shell key renamed inside $(...)"
                );
            }
            let expected_name = if was_renamed {
                Some(ref_key)
            } else {
                orig_name
            };
            assert_eq!(br.kind, ar.kind);
            assert_eq!(br.closed, ar.closed);
            assert_eq!(br.escaped, ar.escaped);
            assert_eq!(br.operator, ar.operator);
            assert_eq!(br.depth, ar.depth);
            assert_eq!(br.parent, ar.parent);
            assert_eq!(br.quote, ar.quote);
            assert!(br.braced == ar.braced || (was_renamed && !br.braced && ar.braced));
            assert_eq!(new_name, expected_name);
            assert_eq!(ar.span, mapped_range(br.span.clone(), &renamed));
            assert_eq!(
                ar.command,
                br.command.clone().map(|r| mapped_range(r, &renamed))
            );
            assert_eq!(
                ar.operator_span,
                br.operator_span
                    .clone()
                    .map(|r| mapped_range_after(r, &renamed))
            );
            assert_eq!(
                ar.argument,
                br.argument.clone().map(|r| mapped_range_after(r, &renamed))
            );
            if br.braced == ar.braced {
                assert_eq!(ar.name, br.name.clone().map(|r| mapped_range(r, &renamed)));
            }
        }
        if !had_matching_ref {
            assert_eq!(after_entry.value(), before_entry.value());
        }
    }
    let mut line_count = 0usize;
    let mut pending_break = false;
    let mut pending_cr = false;
    for &byte in renamed.as_str().as_bytes() {
        if pending_cr {
            pending_cr = false;
            if byte == b'\n' {
                pending_break = true;
                continue;
            }
        }
        if line_count == 0 || pending_break {
            pending_break = false;
            line_count += 1;
        }
        if byte == b'\n' {
            pending_break = true;
        } else if byte == b'\r' && dialect.line_breaks == adotenv::LineBreaks::Universal {
            pending_cr = true;
            pending_break = true;
        }
    }
    let exact_lines = document.rename_with_references_with_options(
        entry.key(),
        ref_key,
        EditOptions::new().with_max_output_lines(line_count),
    );
    assert_eq!(exact_lines, Ok(renamed.clone()));
    if line_count > 0 {
        let too_few_lines = document.rename_with_references_with_options(
            entry.key(),
            ref_key,
            EditOptions::new().with_max_output_lines(line_count - 1),
        );
        assert!(matches!(
            too_few_lines,
            Err(EditError::Limit(limit)) if limit.kind() == LimitKind::Lines
        ));
    }
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
    for dialect in [
        Dialect::PORTABLE,
        Dialect::NODE_DOTENV,
        Dialect::PYTHON_DOTENV,
        Dialect::COMPOSE,
        Dialect::DOCKER_RAW,
    ] {
        check_selected(source, value, operation, selector, dialect);
        check_rename_with_references(source, value, selector, dialect);
        if operation % 5 == 4 {
            check_comment_restore(source, selector, dialect);
        }
        let custom = dialect
            .with_quoted_tail(adotenv::QuotedTail::Ignore)
            .with_inline_comments(adotenv::InlineComments::AnyHash)
            .with_quoted_comments(false)
            .with_raw_values(false);
        check_selected(source, value, operation, selector, custom);
    }
    check_rename_with_references(
        source,
        value,
        selector,
        Dialect::PORTABLE
            .with_quoted_tail(adotenv::QuotedTail::Append)
            .with_bom(adotenv::BomPolicy::StripAll),
    );
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
    if insertion
        .as_ref()
        .is_some_and(|(inserted, _)| key_survives(&document, &operations, &inserted.key))
    {
        assert!(
            result.is_err(),
            "insertion duplicated an existing key: {operations:?}"
        );
    }
    if operations.iter().any(|operation| match *operation {
        Edit::Rename { entry, key } => document
            .entry(entry)
            .is_some_and(|entry| entry.key() != key && key_survives(&document, &operations, key)),
        _ => false,
    }) {
        assert!(
            result.is_err(),
            "rename duplicated an existing key: {operations:?}"
        );
    }
    if known_safe(&document, &operations) {
        assert!(
            result.is_ok(),
            "known-safe edit was refused: {operations:?}: {result:?}"
        );
    }
    let Ok(verified) = result else {
        return;
    };
    let exact = document.edit_with_options(
        &operations,
        EditOptions::new().with_max_output_bytes(verified.as_str().len()),
    );
    assert_eq!(exact, Ok(verified.clone()));
    if !verified.as_str().is_empty() {
        let too_small = document.edit_with_options(
            &operations,
            EditOptions::new().with_max_output_bytes(verified.as_str().len() - 1),
        );
        assert!(matches!(
            too_small,
            Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes
        ));
    }
    check_replay(source, &verified);
    check_targets(&document, &operations, &verified);
    let (after, _) = parse(verified.as_str());
    let actual = meanings(&after);
    if let Some((inserted, position)) = insertion {
        let (_, original_index) = insertion_anchor(&document, key, position).unwrap();
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
