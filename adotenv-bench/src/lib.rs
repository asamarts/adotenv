//! Deterministic benchmark fixtures for `adotenv`.
//!
//! Fixture names, sizes, shapes and hashes are stable within a benchmark
//! corpus revision. A later benchmark mode must consume these same names rather
//! than generating its own inputs.

use std::fmt::Write as _;

use adotenv::parse;
use sha2::{Digest, Sha256};

/// The fixed seed represented in generated fixture metadata.
///
/// Generation is deterministic and does not currently use randomness.
pub const SEED: u64 = 0x0ad0_7e17;

/// A fixture size in declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Size {
    /// Eight declarations.
    Eight,
    /// 128 declarations.
    Typical,
    /// 1,000 declarations.
    OneK,
    /// 10,000 declarations.
    TenK,
    /// 100,000 declarations.
    HundredK,
}

impl Size {
    /// Every benchmark size, from smallest to largest.
    pub const ALL: [Self; 5] = [
        Self::Eight,
        Self::Typical,
        Self::OneK,
        Self::TenK,
        Self::HundredK,
    ];

    /// Returns the target declaration count.
    #[must_use]
    pub const fn count(self) -> usize {
        match self {
            Self::Eight => 8,
            Self::Typical => 128,
            Self::OneK => 1_000,
            Self::TenK => 10_000,
            Self::HundredK => 100_000,
        }
    }

    /// Returns the stable report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Eight => "8",
            Self::Typical => "128",
            Self::OneK => "1k",
            Self::TenK => "10k",
            Self::HundredK => "100k",
        }
    }
}

/// A fixture syntax shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    /// Plain assignments.
    Plain,
    /// Quoted values with escapes and Unicode.
    QuotedEscaped,
    /// Comments, `export` prefixes and bare declarations.
    CommentsExportBare,
    /// Repeated keys.
    Duplicate,
    /// Recovering-parser invalid lines between valid declarations.
    Malformed,
    /// Physical multiline values.
    Multiline,
    /// A BOM and CRLF line endings.
    CrlfBom,
    /// Long values.
    LongLine,
    /// Literal reference operators.
    References,
    /// A mixed, realistic block derived from conformance cases.
    Realistic,
    /// An empty document.
    Empty,
}

impl Shape {
    /// Every fixture shape.
    pub const ALL: [Self; 11] = [
        Self::Plain,
        Self::QuotedEscaped,
        Self::CommentsExportBare,
        Self::Duplicate,
        Self::Malformed,
        Self::Multiline,
        Self::CrlfBom,
        Self::LongLine,
        Self::References,
        Self::Realistic,
        Self::Empty,
    ];

    /// Returns the stable report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::QuotedEscaped => "quoted-escaped",
            Self::CommentsExportBare => "comments-export-bare",
            Self::Duplicate => "duplicate",
            Self::Malformed => "malformed",
            Self::Multiline => "multiline",
            Self::CrlfBom => "crlf-bom",
            Self::LongLine => "long-line",
            Self::References => "references",
            Self::Realistic => "realistic",
            Self::Empty => "empty",
        }
    }
}

/// One deterministic fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    size: Size,
    shape: Shape,
    source: String,
    entries: usize,
    sha256: String,
}

impl Fixture {
    /// Returns the fixture size.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Returns the fixture shape.
    #[must_use]
    pub const fn shape(&self) -> Shape {
        self.shape
    }

    /// Returns the stable fixture name.
    #[must_use]
    pub fn name(&self) -> String {
        format!("{}-{}", self.shape.label(), self.size.label())
    }

    /// Returns the generated source text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the number of declarations the recovering parser found.
    #[must_use]
    pub const fn entries(&self) -> usize {
        self.entries
    }

    /// Returns the source byte length.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.source.len()
    }

    /// Returns the SHA-256 of the source as lowercase hexadecimal.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Builds one deterministic fixture.
#[must_use]
pub fn fixture(size: Size, shape: Shape) -> Fixture {
    let source = source(size, shape);
    let entries = parse(&source).0.entries().count();
    let digest = Sha256::digest(source.as_bytes());
    let mut sha256 = String::with_capacity(64);
    for byte in digest {
        write!(&mut sha256, "{byte:02x}").expect("writing to String cannot fail");
    }
    Fixture {
        size,
        shape,
        source,
        entries,
        sha256,
    }
}

/// Returns every size/shape fixture pair.
pub fn fixtures() -> impl Iterator<Item = Fixture> {
    Size::ALL.into_iter().flat_map(|size| {
        Shape::ALL
            .into_iter()
            .map(move |shape| fixture(size, shape))
    })
}

fn source(size: Size, shape: Shape) -> String {
    let count = size.count();
    match shape {
        Shape::Plain => plain(count),
        Shape::QuotedEscaped => quoted_escaped(count),
        Shape::CommentsExportBare => comments_export_bare(count),
        Shape::Duplicate => duplicate(count),
        Shape::Malformed => malformed(count),
        Shape::Multiline => multiline(count),
        Shape::CrlfBom => crlf_bom(count),
        Shape::LongLine => long_line(count),
        Shape::References => references(count),
        Shape::Realistic => realistic(count),
        Shape::Empty => String::new(),
    }
}

fn key(index: usize) -> String {
    format!("KEY_{index:06}")
}

fn plain(count: usize) -> String {
    let mut source = String::with_capacity(count * 24);
    for index in 0..count {
        writeln!(source, "{}={}", key(index), index).expect("writing to String cannot fail");
    }
    source
}

fn quoted_escaped(count: usize) -> String {
    let mut source = String::with_capacity(count * 64);
    for index in 0..count {
        writeln!(
            source,
            "{}=\"value with spaces #{index} café 🦀 \\n \\t \\\\\"",
            key(index)
        )
        .expect("writing to String cannot fail");
    }
    source
}

fn comments_export_bare(count: usize) -> String {
    let mut source = String::with_capacity(count * 32);
    for index in 0..count {
        if index % 5 == 0 {
            writeln!(source, "# group {index}").expect("writing to String cannot fail");
        }
        if index % 7 == 0 {
            writeln!(source, "{}", key(index)).expect("writing to String cannot fail");
        } else if index % 3 == 0 {
            writeln!(source, "export {}={}", key(index), index)
                .expect("writing to String cannot fail");
        } else {
            writeln!(source, "{}={}", key(index), index).expect("writing to String cannot fail");
        }
    }
    source
}

fn duplicate(count: usize) -> String {
    let mut source = String::with_capacity(count * 24);
    for index in 0..count {
        let key = key(index / 2);
        writeln!(source, "{key}=first-{index}").expect("writing to String cannot fail");
        if index % 2 == 1 {
            writeln!(source, "{key}=last-{index}").expect("writing to String cannot fail");
        }
    }
    source
}

fn malformed(count: usize) -> String {
    let mut source = String::with_capacity(count * 36);
    for index in 0..count {
        if index % 10 == 0 {
            writeln!(source, "BAD KEY_{index}=retained").expect("writing to String cannot fail");
        } else {
            writeln!(source, "{}={}", key(index), index).expect("writing to String cannot fail");
        }
    }
    source
}

fn multiline(count: usize) -> String {
    let mut source = String::with_capacity(count * 40);
    for index in 0..count {
        if index % 8 == 0 {
            writeln!(
                source,
                "{}='line one {index}\nline two {index}'",
                key(index)
            )
            .expect("writing to String cannot fail");
        } else {
            writeln!(source, "{}={}", key(index), index).expect("writing to String cannot fail");
        }
    }
    source
}

fn crlf_bom(count: usize) -> String {
    let mut source = String::with_capacity(count * 26 + 3);
    source.push('\u{feff}');
    for index in 0..count {
        write!(source, "{}={}\r\n", key(index), index).expect("writing to String cannot fail");
    }
    source
}

fn long_line(count: usize) -> String {
    let mut source = String::with_capacity(count * 100);
    let value = "x".repeat(64);
    for index in 0..count {
        writeln!(source, "{}={value}", key(index)).expect("writing to String cannot fail");
    }
    source
}

fn references(count: usize) -> String {
    let mut source = String::with_capacity(count * 50);
    for index in 0..count {
        let value = if index == 0 {
            "${MISSING:-fallback}".to_owned()
        } else {
            format!("${{KEY:-fallback}}/$KEY_{index:06}")
        };
        writeln!(source, "{}={value}", key(index)).expect("writing to String cannot fail");
    }
    source
}

fn realistic(count: usize) -> String {
    let mut source = String::with_capacity(count * 60 + 256);
    source.push_str(
        "# realistic conformance-derived fixture\n\
         export APP_NAME=\"demo café 🦀\"\n\
         DATABASE_URL=postgres://localhost/app\n\
         FEATURE_FLAG=true\n\
         EMPTY=\n\
         BARE\n\
         QUOTED='value with # and spaces'\n\
         MULTILINE='line one\nline two'\n\
         REFERENCE=${APP_NAME:-fallback}\n",
    );
    for index in 0..count.saturating_sub(8) {
        if index % 17 == 0 {
            writeln!(source, "# batch {index}").expect("writing to String cannot fail");
        }
        match index % 9 {
            0 => writeln!(source, "export REAL_{index:06}=\"café 🦀 {index}\""),
            1 => writeln!(source, "REAL_{index:06}='quoted value {index}'"),
            2 => writeln!(source, "REAL_{index:06}=${{APP_NAME:-fallback}}/{index}"),
            3 => writeln!(source, "REAL_{index:06}=plain-{index}"),
            4 => writeln!(source, "REAL_{index:06}="),
            5 => writeln!(source, "REAL_{index:06}=value #{index}"),
            6 => writeln!(source, "REAL_{index:06}='line one\nline two {index}'"),
            7 => writeln!(source, "REAL_{index:06}=value-{index}"),
            _ => writeln!(source, "REAL_{index:06}={index}"),
        }
        .expect("writing to String cannot fail");
    }
    source
}

#[cfg(test)]
mod tests {
    use super::*;
    use adotenv::{parse_values, ItemKind};

    #[test]
    fn fixture_names_are_stable_and_unique() {
        let mut names = std::collections::BTreeSet::new();
        for size in Size::ALL {
            for shape in Shape::ALL {
                names.insert(format!("{}-{}", shape.label(), size.label()));
            }
        }
        assert_eq!(names.len(), Size::ALL.len() * Shape::ALL.len());
    }

    #[test]
    fn generation_is_repeatable_and_hashes_are_sha256() {
        for size in [Size::Eight, Size::Typical, Size::OneK] {
            for shape in Shape::ALL {
                let first = fixture(size, shape);
                let second = fixture(size, shape);
                assert_eq!(first, second);
                assert_eq!(first.sha256().len(), 64);
                assert_eq!(first.source().len(), first.bytes());
                assert_eq!(parse(first.source()).0.entries().count(), first.entries());
            }
        }
        let hundred_k = fixture(Size::HundredK, Shape::Plain);
        assert_eq!(hundred_k.entries(), Size::HundredK.count());
    }

    #[test]
    fn valid_shapes_parse_values_without_errors() {
        for size in [Size::Eight, Size::Typical] {
            for shape in [
                Shape::Plain,
                Shape::QuotedEscaped,
                Shape::CommentsExportBare,
                Shape::Duplicate,
                Shape::Multiline,
                Shape::CrlfBom,
                Shape::LongLine,
                Shape::References,
                Shape::Realistic,
                Shape::Empty,
            ] {
                let fixture = fixture(size, shape);
                if shape != Shape::Empty {
                    assert!(parse_values(fixture.source()).is_ok(), "{}", fixture.name());
                }
            }
        }
    }

    #[test]
    fn malformed_fixture_retains_invalid_items() {
        let fixture = fixture(Size::Eight, Shape::Malformed);
        let (document, _) = parse(fixture.source());
        assert!(document
            .items()
            .any(|item| item.kind() == ItemKind::Invalid));
        assert!(parse_values(fixture.source()).is_err());
    }

    #[test]
    fn known_fixture_hash_is_stable() {
        let fixture = fixture(Size::Eight, Shape::Plain);
        assert_eq!(
            fixture.sha256(),
            "6ee242e0e29423ccf125f7433fe4f3da4bcb73bd9a338e01c07ae55c2391a0cf"
        );
    }
}
