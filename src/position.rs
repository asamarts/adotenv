use alloc::vec::Vec;
use core::fmt;

/// The unit used for a zero-based source column.
///
/// Columns count source units, not terminal display cells. Tabs count as one
/// scalar or UTF-16 unit, and combining characters count separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColumnEncoding {
    /// Bytes, including offsets inside a multi-byte UTF-8 character.
    Utf8,
    /// Unicode scalar values (`char`s).
    Scalar,
    /// UTF-16 code units; supplementary characters occupy two units.
    Utf16,
}

/// A zero-based physical source line and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    /// The zero-based physical line number.
    pub line: usize,
    /// The zero-based column in the selected [`ColumnEncoding`].
    pub column: usize,
}

impl Position {
    /// Explicitly converts both coordinates to one-based display coordinates.
    ///
    /// The returned value is for display, not input to [`LineIndex::to_offset`],
    /// which always expects zero-based coordinates. Returns
    /// [`PositionError::OutOfBounds`] if either addition overflows.
    ///
    /// # Errors
    /// Returns [`PositionError::OutOfBounds`] on coordinate overflow.
    pub fn one_based(self) -> Result<Self, PositionError> {
        Ok(Self {
            line: self.line.checked_add(1).ok_or(PositionError::OutOfBounds)?,
            column: self
                .column
                .checked_add(1)
                .ok_or(PositionError::OutOfBounds)?,
        })
    }
}

/// A source position that cannot be represented in the requested encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PositionError {
    /// An offset, line, or column is outside the source, or an addition overflows.
    OutOfBounds,
    /// A scalar or UTF-16 conversion starts inside a UTF-8 character.
    InvalidUtf8Boundary,
    /// A UTF-16 column splits a supplementary character's surrogate pair.
    SplitSurrogate,
    /// An offset lies between the carriage return and line feed of CRLF.
    InsideCrLf,
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutOfBounds => "source position is out of bounds",
            Self::InvalidUtf8Boundary => "source offset is not a UTF-8 character boundary",
            Self::SplitSurrogate => "UTF-16 column splits a surrogate pair",
            Self::InsideCrLf => "source offset lies inside a CRLF line ending",
        })
    }
}

impl core::error::Error for PositionError {}

#[derive(Debug, Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
}

/// A physical-line index borrowing its original source text.
///
/// LF, CRLF, and lone CR each end a line, independently of parser syntax.
/// A terminal ending creates a final empty line; empty input has one empty
/// line. Line terminators do not contribute to columns: their first byte maps
/// to the preceding line's content end, while the interior of CRLF is rejected.
/// EOF is always a valid offset.
///
/// Construction stores only line boundaries, never a copy of the source.
/// Offset lookup uses binary search; scalar and UTF-16 conversions scan only
/// the selected line's relevant content. No conversion clamps its input.
#[derive(Debug, Clone)]
pub struct LineIndex<'src> {
    source: &'src str,
    lines: Vec<Line>,
}

impl<'src> LineIndex<'src> {
    /// Indexes physical lines in `source`, borrowing it for the index's lifetime.
    #[must_use]
    pub fn new(source: &'src str) -> Self {
        let bytes = source.as_bytes();
        let mut lines = Vec::new();
        let mut start = 0;
        let mut offset = 0;
        while offset < bytes.len() {
            match bytes[offset] {
                b'\r' | b'\n' => {
                    lines.push(Line { start, end: offset });
                    if bytes[offset] == b'\r' && bytes.get(offset + 1) == Some(&b'\n') {
                        offset += 2;
                    } else {
                        offset += 1;
                    }
                    start = offset;
                }
                _ => offset += 1,
            }
        }
        lines.push(Line {
            start,
            end: source.len(),
        });
        Self { source, lines }
    }

    /// Returns the number of physical lines, including a terminal empty line.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Converts a byte offset to a zero-based position.
    ///
    /// UTF-8 columns preserve every byte offset, even within a character.
    /// Scalar and UTF-16 columns require a character boundary. Offsets past EOF
    /// and the interior of a CRLF ending are errors in every encoding.
    ///
    /// # Errors
    /// Rejects offsets beyond EOF, inside CRLF, or inside UTF-8 characters when
    /// the selected encoding requires scalar boundaries.
    pub fn to_position(
        &self,
        offset: usize,
        encoding: ColumnEncoding,
    ) -> Result<Position, PositionError> {
        if offset > self.source.len() {
            return Err(PositionError::OutOfBounds);
        }
        // There is always a line starting at zero, so subtraction cannot underflow.
        let line = self.lines.partition_point(|line| line.start <= offset) - 1;
        let bounds = self.lines[line];
        if offset > bounds.end {
            return Err(PositionError::InsideCrLf);
        }
        let column = match encoding {
            ColumnEncoding::Utf8 => offset - bounds.start,
            ColumnEncoding::Scalar | ColumnEncoding::Utf16 => {
                if !self.source.is_char_boundary(offset) {
                    return Err(PositionError::InvalidUtf8Boundary);
                }
                let prefix = &self.source[bounds.start..offset];
                match encoding {
                    ColumnEncoding::Scalar => prefix.chars().count(),
                    ColumnEncoding::Utf16 => prefix.chars().map(char::len_utf16).sum(),
                    ColumnEncoding::Utf8 => unreachable!(),
                }
            }
        };
        Ok(Position { line, column })
    }

    /// Converts a zero-based position to an exact byte offset.
    ///
    /// Columns may reach the content end but never extend into a terminator or
    /// beyond EOF. UTF-8 columns can select interior character bytes. UTF-16
    /// columns that split a surrogate pair return [`PositionError::SplitSurrogate`].
    ///
    /// # Errors
    /// Rejects lines/columns outside content and UTF-16 columns that split a
    /// surrogate pair. Terminators cannot be addressed as additional columns.
    pub fn to_offset(
        &self,
        position: Position,
        encoding: ColumnEncoding,
    ) -> Result<usize, PositionError> {
        let line = self
            .lines
            .get(position.line)
            .ok_or(PositionError::OutOfBounds)?;
        let content = &self.source[line.start..line.end];
        if position.column > content.len() {
            return Err(PositionError::OutOfBounds);
        }
        if encoding == ColumnEncoding::Utf8 {
            return Ok(line.start + position.column);
        }
        let mut column = 0;
        for (offset, ch) in content.char_indices() {
            if column == position.column {
                return Ok(line.start + offset);
            }
            column += match encoding {
                ColumnEncoding::Scalar => 1,
                ColumnEncoding::Utf16 => ch.len_utf16(),
                ColumnEncoding::Utf8 => unreachable!(),
            };
            if column > position.column {
                return Err(PositionError::SplitSurrogate);
            }
        }
        if column == position.column {
            Ok(line.end)
        } else {
            Err(PositionError::OutOfBounds)
        }
    }
}
