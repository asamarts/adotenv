use core::fmt;

/// Which configured parse or load limit was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LimitKind {
    /// The source contains more bytes than the configured cap.
    InputBytes,
    /// Parsing reached more items than the configured cap.
    Items,
    /// Parsing produced more diagnostics than the configured cap.
    Diagnostics,
}

impl LimitKind {
    /// Returns the stable lowercase name of the limit kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InputBytes => "input-bytes",
            Self::Items => "items",
            Self::Diagnostics => "diagnostics",
        }
    }
}

impl fmt::Display for LimitKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A configured limit and the value that exceeded it.
///
/// The values are sizes and counts, never document contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LimitExceeded {
    kind: LimitKind,
    limit: usize,
    observed: usize,
}

impl LimitExceeded {
    pub(crate) const fn new(kind: LimitKind, limit: usize, observed: usize) -> Self {
        Self {
            kind,
            limit,
            observed,
        }
    }

    /// Returns which limit was exceeded.
    #[must_use]
    pub const fn kind(self) -> LimitKind {
        self.kind
    }

    /// Returns the configured maximum.
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }

    /// Returns the size or count that was observed.
    #[must_use]
    pub const fn observed(self) -> usize {
        self.observed
    }
}

impl fmt::Display for LimitExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} limit {} exceeded with observed {}",
            self.kind, self.limit, self.observed
        )
    }
}
