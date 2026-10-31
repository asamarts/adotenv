//! Comment-aware key sorting within blank-line groups that keeps resolved
//! values unchanged.
//!
//! Sorting is a separate verified transformation from trivia formatting. It
//! reorders whole group chunks so leading comments and inline comments travel
//! with their entries. Keys that reference each other keep their relative
//! order. A group is left untouched when it contains malformed data, duplicate
//! or empty keys, a self-reference or a reference cycle.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec,
    vec::Vec,
};

use crate::{
    AssignmentState, Document, Entry, FormatError, FormatResult, Item, ItemKind, LineEnding, Quote,
    ReferenceKind, TextEdit,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryFacts {
    value: Option<String>,
    state: AssignmentState,
    quote: Quote,
    exported: bool,
    participates: bool,
    inline_comment: Option<String>,
}

struct Plan {
    index: usize,
    range: core::ops::Range<usize>,
    text: String,
    keys: Vec<String>,
}

impl Document<'_> {
    /// Sort declarations within each blank-line-separated group by key.
    ///
    /// Comments before the first entry of a group stay at the top as a group
    /// header. Comments immediately before later entries move with those entries.
    /// Comments after the last entry stay at the bottom. Inline comments are part
    /// of the moved entry.
    ///
    /// When one key's value references another key of the same group (`$VAR`
    /// or `${VAR...}`, including nested and quoted forms), the two keys keep
    /// their current relative order. Every other key is ordered by key, so the
    /// result is the key order that respects those pairs. Which declaration a
    /// reference sees does not change, and [`Document::resolve`] returns the
    /// same values before and after sorting under sequential and whole-file
    /// scope. References to keys outside the group are not ordering
    /// constraints.
    ///
    /// Groups with empty/duplicate keys, malformed or ignored items, a
    /// self-reference or a reference cycle are left byte-for-byte unchanged, as
    /// are groups where a declaration shares its physical line with the next
    /// statement.
    ///
    /// Sorting never adds or removes a final line ending. When the file ends
    /// without one and its last declaration moves up, that declaration takes
    /// the line ending of the declaration that becomes last.
    ///
    /// The result is reparsed and verified before it is returned.
    ///
    /// # Errors
    /// Returns [`FormatError::WouldChangeMeaning`] if the generated text cannot
    /// be verified.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::parse_strict;
    ///
    /// // B references A and stays above it; the other keys are sorted.
    /// let doc = parse_strict("D=4\nB=${A}\nC=3\nA=1\n").unwrap();
    /// let sorted = doc.sort().unwrap();
    /// assert_eq!(sorted.as_str(), "B=${A}\nA=1\nC=3\nD=4\n");
    ///
    /// let doc = parse_strict("B=2\nA=1").unwrap();
    /// assert_eq!(doc.sort().unwrap().as_str(), "A=1\nB=2");
    /// ```
    pub fn sort(&self) -> Result<FormatResult, FormatError> {
        let mut plans = Vec::new();
        for (index, group) in self.groups().enumerate() {
            if let Some(plan) = self.plan_group(index, group) {
                plans.push(plan);
            }
        }
        if plans.is_empty() {
            return Ok(FormatResult::from_parts(
                Vec::new(),
                self.source().to_string(),
            ));
        }

        let mut output = String::with_capacity(self.source().len());
        let mut cursor = 0;
        let mut edits = Vec::with_capacity(plans.len());
        for plan in &plans {
            output.push_str(&self.source()[cursor..plan.range.start]);
            output.push_str(&plan.text);
            cursor = plan.range.end;
            edits.push(TextEdit {
                range: plan.range.clone(),
                replacement: plan.text.clone(),
            });
        }
        output.push_str(&self.source()[cursor..]);

        let (parsed, _) = crate::parse_with(&output, self.dialect());
        if !verify(self, &parsed, &plans) {
            return Err(FormatError::WouldChangeMeaning);
        }
        Ok(FormatResult::from_parts(edits, output))
    }

    fn plan_group(&self, index: usize, group: crate::Group<'_>) -> Option<Plan> {
        let items: Vec<Item<'_>> = group.items().collect();
        let entries: Vec<Entry<'_>> = items.iter().filter_map(|item| item.entry()).collect();
        if entries.len() < 2
            || items
                .iter()
                .any(|item| !matches!(item.kind(), ItemKind::Entry | ItemKind::Comment))
        {
            return None;
        }

        let first_entry = items
            .iter()
            .position(|item| item.kind() == ItemKind::Entry)?;
        let last_entry = items
            .iter()
            .rposition(|item| item.kind() == ItemKind::Entry)?;
        let source = self.source();
        let mut key_index = BTreeMap::new();
        let mut keys = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let key = entry.key();
            if key.is_empty() || key_index.insert(key.to_string(), index).is_some() {
                return None;
            }
            keys.push(key.to_string());
        }

        let mut chunks = Vec::with_capacity(entries.len());
        let entry_positions: Vec<_> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind() == ItemKind::Entry)
            .map(|(position, _)| position)
            .collect();
        for (entry_ordinal, &position) in entry_positions.iter().enumerate() {
            let mut start = position;
            if entry_ordinal != 0 {
                while start != 0 && items[start - 1].kind() == ItemKind::Comment {
                    start -= 1;
                }
            }
            let start = items[start].span().start;
            let item = items[position];
            chunks.push(&source[start..item.span().end]);
        }
        // A declaration that shares its physical line with the next statement
        // cannot move without joining lines. Only the group's last declaration
        // may lack a line ending, and only at the end of the file.
        let endings: Vec<_> = entry_positions
            .iter()
            .map(|&position| items[position].line_ending())
            .collect();
        let last = entries.len() - 1;
        if endings[..last].contains(&LineEnding::None) {
            return None;
        }
        let open_end = endings[last] == LineEnding::None;
        if open_end && items[last_entry].span().end != source.len() {
            return None;
        }

        let order = reference_order(&entries, &keys, &key_index, source)?;

        let header = &source[group.span().start..items[first_entry].span().start];
        let trailing = &source[items[last_entry].span().end..group.span().end];
        // When the file ends without a line ending and its last declaration
        // moves up, that declaration takes the line ending of the declaration
        // that becomes last, so the file still has no final line ending.
        let moved_ending = order
            .last()
            .filter(|&&final_entry| open_end && final_entry != last)
            .map(|&final_entry| endings[final_entry].as_str());
        let mut text = String::new();
        text.push_str(header);
        for (position, &entry) in order.iter().enumerate() {
            let chunk = chunks[entry];
            match moved_ending {
                Some(ending) if position + 1 == order.len() => {
                    text.push_str(&chunk[..chunk.len() - ending.len()]);
                }
                Some(ending) if entry == last => {
                    text.push_str(chunk);
                    text.push_str(ending);
                }
                _ => text.push_str(chunk),
            }
        }
        text.push_str(trailing);
        if text == source[group.span()] {
            return None;
        }
        Some(Plan {
            index,
            range: group.span(),
            text,
            keys: order.into_iter().map(|entry| keys[entry].clone()).collect(),
        })
    }
}

// The sorted order of a group's entries by source ordinal, or `None` for a
// self-reference or a reference cycle.
fn reference_order(
    entries: &[Entry<'_>],
    keys: &[String],
    key_index: &BTreeMap<String, usize>,
    source: &str,
) -> Option<Vec<usize>> {
    // (dependent, dependency) pairs between entries of this group, by
    // source ordinal. Every reference candidate counts, eligible or not,
    // so the order is safe under any reference rules.
    let mut references = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        for reference in entry.references() {
            if reference.kind != ReferenceKind::Variable {
                continue;
            }
            let Some(name) = reference.name else {
                continue;
            };
            let Some(&dependency) = key_index.get(&source[name]) else {
                continue;
            };
            if dependency == index {
                return None;
            }
            references.push((index, dependency));
        }
    }
    // Reordering a referencing pair changes which declaration a sequential
    // reference sees, so each pair keeps its source order. Those edges all
    // point forward in source order, so a constrained order always exists.
    // Reference cycles still leave the group unchanged.
    let mut dependencies = vec![Vec::new(); entries.len()];
    let mut constraints = vec![Vec::new(); entries.len()];
    for &(dependent, dependency) in &references {
        dependencies[dependency].push(dependent);
        constraints[dependent.min(dependency)].push(dependent.max(dependency));
    }
    if topological_order(&dependencies, keys).len() != entries.len() {
        return None;
    }
    Some(topological_order(&constraints, keys))
}

// Kahn's algorithm, always taking the smallest ready key. A result shorter
// than `keys` means the edges contain a cycle.
fn topological_order(edges: &[Vec<usize>], keys: &[String]) -> Vec<usize> {
    let mut indegree = vec![0usize; keys.len()];
    for targets in edges {
        for &target in targets {
            indegree[target] += 1;
        }
    }
    let mut ready = BTreeSet::new();
    for (index, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.insert((keys[index].as_str(), index));
        }
    }
    let mut order = Vec::with_capacity(keys.len());
    while let Some((_, index)) = ready.pop_first() {
        order.push(index);
        for &target in &edges[index] {
            indegree[target] -= 1;
            if indegree[target] == 0 {
                ready.insert((keys[target].as_str(), target));
            }
        }
    }
    order
}

fn facts(entry: Entry<'_>) -> EntryFacts {
    EntryFacts {
        value: entry.value().map(ToString::to_string),
        state: entry.state(),
        quote: entry.quote(),
        exported: entry.exported(),
        participates: entry.participates_in_lookup(),
        inline_comment: entry.inline_comment().map(ToString::to_string),
    }
}

fn verify(original: &Document<'_>, output: &Document<'_>, plans: &[Plan]) -> bool {
    if original.has_bom() != output.has_bom()
        || original.groups().count() != output.groups().count()
    {
        return false;
    }
    let mut plans_iter = plans.iter().peekable();
    for (index, (before, after)) in original.groups().zip(output.groups()).enumerate() {
        let plan = if plans_iter.peek().is_some_and(|plan| plan.index == index) {
            plans_iter.next()
        } else {
            None
        };
        let Some(plan) = plan else {
            if original.source()[before.span()] != output.source()[after.span()] {
                return false;
            }
            continue;
        };
        if output.source()[after.span()] != plan.text
            || after.entries().map(Entry::key).collect::<Vec<_>>() != plan.keys
            || before.items().count() != after.items().count()
        {
            return false;
        }
        let before_meanings: BTreeMap<_, _> = before
            .entries()
            .map(|entry| (entry.key().to_string(), facts(entry)))
            .collect();
        let after_meanings: BTreeMap<_, _> = after
            .entries()
            .map(|entry| (entry.key().to_string(), facts(entry)))
            .collect();
        if before_meanings != after_meanings {
            return false;
        }
        let mut before_comments: Vec<_> = before.comments().map(Item::raw).collect();
        let mut after_comments: Vec<_> = after.comments().map(Item::raw).collect();
        before_comments.sort_unstable();
        after_comments.sort_unstable();
        if before_comments != after_comments {
            return false;
        }
    }
    plans_iter.next().is_none()
}
