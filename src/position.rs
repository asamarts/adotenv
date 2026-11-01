use alloc::vec::Vec;
use core::fmt;

/// The unit that a [`Position`] column counts.
///
/// The same byte offset has a different column in each unit when the line
/// holds characters outside ASCII. Pick the unit your consumer expects: byte
/// columns for Rust string slicing, scalar columns for counting `char`s, and
/// UTF-16 columns for the Language Server Protocol and JavaScript editors.
/// No unit is a terminal display width: a tab counts as one unit, and a
/// combining accent counts as a character of its own.
///
/// # Examples
///
/// ```
/// use adotenv::{ColumnEncoding, LineIndex};
///
/// // `é` takes 2 bytes and `😀` takes 4 bytes or 2 UTF-16 units.
/// let source = "NAME=é😀!\n";
/// let index = LineIndex::new(source);
/// let offset = source.find('!').unwrap();
///
/// assert_eq!(offset, 11);
/// assert_eq!(index.to_position(offset, ColumnEncoding::Utf8)?.column, 11);
/// assert_eq!(index.to_position(offset, ColumnEncoding::Scalar)?.column, 7);
/// assert_eq!(index.to_position(offset, ColumnEncoding::Utf16)?.column, 8);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColumnEncoding {
    /// Bytes of UTF-8.
    ///
    /// A byte column can point inside a multi-byte character.
    Utf8,
    /// Unicode scalar values, the units of Rust's `char`.
    Scalar,
    /// UTF-16 code units.
    ///
    /// A character outside the Basic Multilingual Plane, such as most emoji,
    /// takes two units (a surrogate pair).
    Utf16,
}

/// A zero-based line and column in the source.
///
/// [`LineIndex::to_position`] returns one and [`LineIndex::to_offset`] takes
/// one; the column unit is the [`ColumnEncoding`] passed to them. Both
/// coordinates start at 0. Editors and messages usually count from 1: convert
/// with [`Position::one_based`] for display. Lines in [`Entry::line`] and
/// [`Diagnostic::line`] are already one-based.
///
/// [`Entry::line`]: crate::Entry::line
/// [`Diagnostic::line`]: crate::Diagnostic::line
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    /// The zero-based line.
    pub line: usize,
    /// The zero-based column, in the unit of the [`ColumnEncoding`] used.
    pub column: usize,
}

impl Position {
    /// Returns the position with one added to the line and to the column, for
    /// display.
    ///
    /// The result is for showing to people. Do not pass it to
    /// [`LineIndex::to_offset`], which expects zero-based coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`PositionError::OutOfBounds`] if the line or the column is
    /// `usize::MAX`, so adding one would overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{ColumnEncoding, LineIndex, Position};
    ///
    /// let source = "HOST=localhost\nPORT=8080\n";
    /// let index = LineIndex::new(source);
    /// let position = index.to_position(source.find("8080").unwrap(), ColumnEncoding::Scalar)?;
    /// assert_eq!(position, Position { line: 1, column: 5 });
    ///
    /// let display = position.one_based()?;
    /// assert_eq!(format!("{}:{}", display.line, display.column), "2:6");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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

/// Why a byte offset or position cannot be converted.
///
/// [`LineIndex`] returns this error instead of rounding or clamping a value
/// that does not name an exact place in the source.
/// [`Entry::decoded_offset_to_raw`](crate::Entry::decoded_offset_to_raw) also
/// uses it.
///
/// # Examples
///
/// ```
/// use adotenv::{ColumnEncoding, LineIndex, Position, PositionError};
///
/// let index = LineIndex::new("A=1\r\nB=é\n");
///
/// // Byte 4 is between the CR and the LF.
/// assert_eq!(
///     index.to_position(4, ColumnEncoding::Utf8),
///     Err(PositionError::InsideCrLf)
/// );
/// // Byte 8 is inside the two-byte `é`.
/// assert_eq!(
///     index.to_position(8, ColumnEncoding::Scalar),
///     Err(PositionError::InvalidUtf8Boundary)
/// );
/// // Line 0 has 3 bytes before its line ending, so column 4 is past its end.
/// assert_eq!(
///     index.to_offset(Position { line: 0, column: 4 }, ColumnEncoding::Utf8),
///     Err(PositionError::OutOfBounds)
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PositionError {
    /// The offset, line or column is past the end, or adding one to a
    /// coordinate would overflow.
    OutOfBounds,
    /// The offset is inside a UTF-8 character.
    ///
    /// Scalar and UTF-16 columns can only describe offsets at character
    /// boundaries.
    InvalidUtf8Boundary,
    /// The UTF-16 column falls between the two units of a surrogate pair,
    /// inside a character such as an emoji.
    SplitSurrogate,
    /// The offset is between the CR and the LF of a CRLF line ending.
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

/// Converts between byte offsets and zero-based line and column positions.
///
/// Spans in this crate are byte offsets. Use a line index to turn them into a
/// [`Position`] for an editor or a message, with columns in the
/// [`ColumnEncoding`] you choose, and back.
///
/// LF, CRLF and a lone CR each end a line, whatever the dialect. A line ending
/// at the end of the source starts one more, empty line, and an empty source
/// has one empty line. A line ending has no column of its own: the offset of
/// its first byte is the end of the line's content, and the offset between
/// the CR and the LF of a CRLF is an error. The end of the source is always a
/// valid offset. Conversions never round or clamp: an offset or position that
/// does not name an exact place returns a [`PositionError`].
///
/// Building the index stores only where lines start and end, not a copy of
/// the source, and borrows the source. Finding a line takes logarithmic time;
/// scalar and UTF-16 columns also scan that line. [`Document::line_index`]
/// builds one for a document.
///
/// [`Document::line_index`]: crate::Document::line_index
///
/// # Examples
///
/// ```
/// use adotenv::{ColumnEncoding, LineIndex, Position};
///
/// let source = "A=1\nEMOJI=😀!\n";
/// let index = LineIndex::new(source);
/// let offset = source.find('!').unwrap();
///
/// assert_eq!(index.line_count(), 3);
/// assert_eq!(
///     index.to_position(offset, ColumnEncoding::Utf8)?,
///     Position { line: 1, column: 10 }
/// );
/// assert_eq!(
///     index.to_position(offset, ColumnEncoding::Utf16)?,
///     Position { line: 1, column: 8 }
/// );
/// assert_eq!(
///     index.to_offset(Position { line: 1, column: 7 }, ColumnEncoding::Scalar)?,
///     offset
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone)]
pub struct LineIndex<'src> {
    source: &'src str,
    lines: Vec<Line>,
}

impl<'src> LineIndex<'src> {
    /// Builds a line index over `source`, borrowing it.
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

    /// Returns the number of lines, counting the empty line after a final line
    /// ending.
    ///
    /// `"A=1\n"` has 2 lines, `"A=1"` has 1, and `""` has 1.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Converts a byte offset in the source to a zero-based line and column.
    ///
    /// With [`ColumnEncoding::Utf8`] every offset in a line has a column, even
    /// one inside a multi-byte character. [`ColumnEncoding::Scalar`] and
    /// [`ColumnEncoding::Utf16`] need the offset to be at a character
    /// boundary. The offset of a line ending's first byte is the column after
    /// the line's last character.
    ///
    /// # Errors
    ///
    /// - [`PositionError::OutOfBounds`] when `offset` is greater than the
    ///   source length.
    /// - [`PositionError::InsideCrLf`] when `offset` is between the CR and the
    ///   LF of a CRLF line ending.
    /// - [`PositionError::InvalidUtf8Boundary`] when the encoding is
    ///   [`ColumnEncoding::Scalar`] or [`ColumnEncoding::Utf16`] and `offset`
    ///   is inside a UTF-8 character.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{ColumnEncoding, LineIndex, Position};
    ///
    /// let index = LineIndex::new("A=1\r\nB=2");
    ///
    /// // Byte 3 is the CR: the end of line 0's content.
    /// assert_eq!(index.to_position(3, ColumnEncoding::Utf8)?, Position { line: 0, column: 3 });
    /// assert_eq!(index.to_position(5, ColumnEncoding::Utf8)?, Position { line: 1, column: 0 });
    /// // The end of the source is a valid offset.
    /// assert_eq!(index.to_position(8, ColumnEncoding::Utf8)?, Position { line: 1, column: 3 });
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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

    /// Converts a zero-based line and column to a byte offset in the source.
    ///
    /// The column can be at most the end of the line's content; a line ending
    /// cannot be addressed as extra columns. With [`ColumnEncoding::Utf8`] a
    /// column can point inside a multi-byte character, and the offset of that
    /// byte is returned.
    ///
    /// # Errors
    ///
    /// - [`PositionError::OutOfBounds`] when the line does not exist or the
    ///   column is past the end of the line's content.
    /// - [`PositionError::SplitSurrogate`] when the encoding is
    ///   [`ColumnEncoding::Utf16`] and the column falls between the two units
    ///   of a surrogate pair.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{ColumnEncoding, LineIndex, Position, PositionError};
    ///
    /// let source = "A=😀\n";
    /// let index = LineIndex::new(source);
    ///
    /// let after = Position { line: 0, column: 4 };
    /// assert_eq!(index.to_offset(after, ColumnEncoding::Utf16)?, source.len() - 1);
    ///
    /// let middle = Position { line: 0, column: 3 };
    /// assert_eq!(
    ///     index.to_offset(middle, ColumnEncoding::Utf16),
    ///     Err(PositionError::SplitSurrogate)
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
