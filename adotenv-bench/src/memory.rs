//! Isolated memory measurements for benchmark cases.
//!
//! Each case is intended to run in a fresh process. The binary installs
//! [`CountingAllocator`] as the global allocator and prints one JSON report.

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use adotenv::{load::LoadedEnvironment, parse_strict, Document, ExpandOptions, ResolvedDocument};

use crate::competitors::{load_values, parse_values, Competitor};
use crate::{fixture, Shape, Size};

/// Allocation counters installed by the memory binary.
pub struct Counters {
    current: AtomicUsize,
    peak: AtomicUsize,
    allocations: AtomicUsize,
}

/// Global allocation counters.
pub static COUNTERS: Counters = Counters::new();

impl Counters {
    const fn new() -> Self {
        Self {
            current: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
        }
    }

    fn allocated(&self, bytes: usize) {
        let current = self.current.fetch_add(bytes, Ordering::Relaxed) + bytes;
        self.peak.fetch_max(current, Ordering::Relaxed);
        self.allocations.fetch_add(1, Ordering::Relaxed);
    }

    fn freed(&self, bytes: usize) {
        self.current.fetch_sub(bytes, Ordering::Relaxed);
    }

    fn current(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    fn peak(&self) -> usize {
        self.peak.load(Ordering::Relaxed)
    }

    fn allocations(&self) -> usize {
        self.allocations.load(Ordering::Relaxed)
    }

    fn reset_peak(&self) {
        self.peak.store(self.current(), Ordering::Relaxed);
    }
}

/// A counting global allocator for the memory benchmark binary.
pub struct CountingAllocator;

// SAFETY: the allocator forwards every operation to the system allocator and
// only records atomic counters; it never dereferences or changes the allocation.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            COUNTERS.allocated(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            COUNTERS.allocated(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        COUNTERS.freed(layout.size());
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            if new_size >= layout.size() {
                COUNTERS.allocated(new_size - layout.size());
            } else {
                COUNTERS.freed(layout.size() - new_size);
            }
            COUNTERS.allocations.fetch_add(1, Ordering::Relaxed);
        }
        new_pointer
    }
}

/// The operation being measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Values-only parsing.
    Values,
    /// Lossless document parsing.
    Document,
    /// File loading.
    Load,
    /// Literal reference resolution.
    Resolve,
}

impl Mode {
    /// Parses a mode label.
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "values" => Some(Self::Values),
            "document" => Some(Self::Document),
            "load" => Some(Self::Load),
            "resolve" => Some(Self::Resolve),
            _ => None,
        }
    }

    /// Returns the stable report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Values => "values",
            Self::Document => "document",
            Self::Load => "load",
            Self::Resolve => "resolve",
        }
    }
}

/// The implementation being measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Implementation {
    /// The adotenv implementation.
    Adotenv,
    /// A pinned competitor.
    Competitor(Competitor),
}

impl Implementation {
    /// Parses an implementation label.
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        if label == "adotenv" {
            return Some(Self::Adotenv);
        }
        Competitor::ALL
            .into_iter()
            .find(|competitor| competitor.label() == label)
            .map(Self::Competitor)
    }

    /// Returns the stable report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Adotenv => "adotenv",
            Self::Competitor(competitor) => competitor.label(),
        }
    }

    /// Returns whether this implementation supports a mode.
    #[must_use]
    pub const fn supports(self, mode: Mode) -> bool {
        match (self, mode) {
            (Self::Adotenv, _) => true,
            (Self::Competitor(competitor), Mode::Values) => competitor.supports_values(),
            (Self::Competitor(competitor), Mode::Load) => competitor.supports_load(),
            (Self::Competitor(_), Mode::Document | Mode::Resolve) => false,
        }
    }
}

/// A memory measurement for one case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryReport {
    /// Fixture size label.
    pub size: &'static str,
    /// Fixture shape label.
    pub shape: &'static str,
    /// Operation label.
    pub mode: &'static str,
    /// Implementation label.
    pub implementation: &'static str,
    /// Fixture entry count.
    pub entries: usize,
    /// Fixture source byte length.
    pub source_bytes: usize,
    /// SHA-256 of the generated fixture source.
    pub fixture_sha256: String,
    /// Peak live heap above the pre-operation baseline.
    pub peak_heap_bytes: usize,
    /// Live heap retained by the operation result before it is dropped.
    pub retained_heap_bytes: usize,
    /// Live heap after dropping the operation result.
    pub heap_after_drop_bytes: usize,
    /// Allocation calls above the pre-operation baseline.
    pub allocations: usize,
    /// Resident set size before the operation, when available.
    pub rss_before_bytes: Option<u64>,
    /// Process peak RSS after the operation, when available.
    pub rss_peak_bytes: Option<u64>,
    /// Resident set size after dropping the result, when available.
    pub rss_after_bytes: Option<u64>,
}

impl MemoryReport {
    /// Renders the report as one Markdown table row.
    #[must_use]
    pub fn markdown(&self) -> String {
        format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            self.size,
            self.shape,
            self.mode,
            self.implementation,
            self.entries,
            self.source_bytes,
            self.peak_heap_bytes,
            self.retained_heap_bytes,
            self.heap_after_drop_bytes,
            self.allocations,
            optional_markdown(self.rss_before_bytes),
            optional_markdown(self.rss_peak_bytes),
            optional_markdown(self.rss_after_bytes)
        )
    }

    /// Renders the report as one JSON object.
    #[must_use]
    pub fn json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"size\":\"{}\",\"shape\":\"{}\",\"mode\":\"{}\",",
                "\"implementation\":\"{}\",\"entries\":{},\"source_bytes\":{},",
                "\"fixture_sha256\":\"{}\",",
                "\"peak_heap_bytes\":{},\"retained_heap_bytes\":{},\"heap_after_drop_bytes\":{},\"allocations\":{},",
                "\"rss_before_bytes\":{},\"rss_peak_bytes\":{},\"rss_after_bytes\":{}",
                "}}"
            ),
            self.size,
            self.shape,
            self.mode,
            self.implementation,
            self.entries,
            self.source_bytes,
            self.fixture_sha256,
            self.peak_heap_bytes,
            self.retained_heap_bytes,
            self.heap_after_drop_bytes,
            self.allocations,
            optional_json(self.rss_before_bytes),
            optional_json(self.rss_peak_bytes),
            optional_json(self.rss_after_bytes)
        )
    }
}

fn optional_json(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn optional_markdown(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| value.to_string())
}

/// Runs one memory case.
///
/// # Errors
///
/// Returns an error for an unsupported implementation/mode, fixture generation,
/// adapter failure, or file setup failure.
pub fn run(
    size: Size,
    shape: Shape,
    mode: Mode,
    implementation: Implementation,
) -> Result<MemoryReport, String> {
    if !implementation.supports(mode) {
        return Err(format!(
            "{} does not support mode {}",
            implementation.label(),
            mode.label()
        ));
    }
    let fixture = fixture(size, shape);
    let path = if mode == Mode::Load {
        let path = temporary_path();
        fs::write(&path, fixture.source()).map_err(|error| error.to_string())?;
        Some(path)
    } else {
        None
    };
    let rss_before = rss_bytes();
    let baseline = COUNTERS.current();
    let allocations_before = COUNTERS.allocations();
    COUNTERS.reset_peak();

    let result = match execute(&fixture, mode, implementation, path.as_deref()) {
        Ok(result) => result,
        Err(error) => {
            if let Some(path) = &path {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
    };
    let peak_heap_bytes = COUNTERS.peak().saturating_sub(baseline);
    let allocations = COUNTERS.allocations().saturating_sub(allocations_before);
    let rss_peak_bytes = rss_peak_bytes();
    let retained_heap_bytes = COUNTERS.current().saturating_sub(baseline);
    drop(result);
    let heap_after_drop_bytes = COUNTERS.current().saturating_sub(baseline);
    let rss_after_bytes = rss_bytes();

    if let Some(path) = path {
        let _ = fs::remove_file(path);
    }
    Ok(MemoryReport {
        size: size.label(),
        shape: shape.label(),
        mode: mode.label(),
        implementation: implementation.label(),
        entries: fixture.entries(),
        source_bytes: fixture.bytes(),
        fixture_sha256: fixture.sha256().to_owned(),
        peak_heap_bytes,
        retained_heap_bytes,
        heap_after_drop_bytes,
        allocations,
        rss_before_bytes: rss_before,
        rss_peak_bytes,
        rss_after_bytes,
    })
}

// The variants are intentionally held only to keep their allocations alive
// while retained memory is measured.
#[allow(dead_code)]
enum Holding<'a> {
    Values(BTreeMap<String, String>),
    Document(Document<'a>),
    Load(LoadedEnvironment),
    Resolve(ResolvedDocument),
}

fn execute<'a>(
    fixture: &'a crate::Fixture,
    mode: Mode,
    implementation: Implementation,
    path: Option<&Path>,
) -> Result<Holding<'a>, String> {
    match (mode, implementation) {
        (Mode::Values, Implementation::Adotenv) => Ok(Holding::Values(
            adotenv::parse_values(fixture.source()).map_err(|error| error.to_string())?,
        )),
        (Mode::Values, Implementation::Competitor(competitor)) => {
            Ok(Holding::Values(parse_values(competitor, fixture.source())?))
        }
        (Mode::Document, Implementation::Adotenv) => Ok(Holding::Document(
            parse_strict(fixture.source()).map_err(|error| error.to_string())?,
        )),
        (Mode::Load, Implementation::Adotenv) => {
            let path = path.ok_or("load case has no path")?;
            Ok(Holding::Load(
                adotenv::load::load_files([path], adotenv::Dialect::PORTABLE)
                    .map_err(|error| error.to_string())?,
            ))
        }
        (Mode::Load, Implementation::Competitor(competitor)) => {
            let path = path.ok_or("load case has no path")?;
            Ok(Holding::Values(load_values(competitor, path)?))
        }
        (Mode::Resolve, Implementation::Adotenv) => {
            let document = parse_strict(fixture.source()).map_err(|error| error.to_string())?;
            Ok(Holding::Resolve(
                document
                    .resolve(|_| None, &ExpandOptions::portable())
                    .map_err(|error| error.to_string())?,
            ))
        }
        (_, implementation) => Err(format!(
            "{} does not support mode {}",
            implementation.label(),
            mode.label()
        )),
    }
}

fn temporary_path() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "adotenv-bench-memory-{}-{}.env",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(target_os = "linux")]
fn rss_bytes() -> Option<u64> {
    rss_field("VmRSS:")
}

#[cfg(target_os = "linux")]
fn rss_peak_bytes() -> Option<u64> {
    rss_field("VmHWM:")
}

#[cfg(not(target_os = "linux"))]
fn rss_bytes() -> Option<u64> {
    None
}

#[cfg(not(target_os = "linux"))]
fn rss_peak_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn rss_field(label: &str) -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kilobytes| kilobytes * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_and_implementation_labels_round_trip() {
        for mode in [Mode::Values, Mode::Document, Mode::Load, Mode::Resolve] {
            assert_eq!(Mode::from_label(mode.label()), Some(mode));
        }
        for competitor in Competitor::ALL {
            let implementation = Implementation::Competitor(competitor);
            assert_eq!(
                Implementation::from_label(implementation.label()),
                Some(implementation)
            );
        }
        assert_eq!(
            Implementation::from_label("adotenv"),
            Some(Implementation::Adotenv)
        );
    }

    #[test]
    fn reports_json_with_missing_rss() {
        let report = MemoryReport {
            size: "8",
            shape: "plain",
            mode: "values",
            implementation: "adotenv",
            entries: 8,
            source_bytes: 64,
            fixture_sha256: "abc123".to_owned(),
            peak_heap_bytes: 128,
            retained_heap_bytes: 64,
            heap_after_drop_bytes: 0,
            allocations: 4,
            rss_before_bytes: None,
            rss_peak_bytes: None,
            rss_after_bytes: None,
        };
        assert_eq!(
            report.json(),
            "{\"size\":\"8\",\"shape\":\"plain\",\"mode\":\"values\",\"implementation\":\"adotenv\",\"entries\":8,\"source_bytes\":64,\"fixture_sha256\":\"abc123\",\"peak_heap_bytes\":128,\"retained_heap_bytes\":64,\"heap_after_drop_bytes\":0,\"allocations\":4,\"rss_before_bytes\":null,\"rss_peak_bytes\":null,\"rss_after_bytes\":null}"
        );
    }
}
