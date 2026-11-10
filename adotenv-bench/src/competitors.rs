//! Pinned competitor adapters and semantic normalization.
//!
//! Only feature-comparable cells are timed. Adapters validate their result
//! against the adotenv fixture before a benchmark is allowed to run.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::Shape;

/// Shapes whose values can be compared across every value-capable adapter.
pub const COMPARABLE_VALUES_SHAPES: [Shape; 3] = [Shape::Plain, Shape::LongLine, Shape::Empty];

/// Shapes whose loaded values can be compared across every load-capable adapter.
pub const COMPARABLE_LOAD_SHAPES: [Shape; 3] = [Shape::Plain, Shape::LongLine, Shape::Empty];

/// Why the current competitor matrix has no comparable resolve cells.
pub const RESOLVE_UNSUPPORTED_REASON: &str =
    "competitor interpolation policies differ from adotenv literal reference resolution";

/// A pinned external `.env` implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Competitor {
    /// The legacy `dotenv` crate.
    Dotenv,
    /// The `dotenv-flow` crate.
    DotenvFlow,
    /// The Pest-based `dotenv-parser` crate.
    DotenvParser,
    /// The `dotenvs` crate.
    Dotenvs,
    /// The `dotenvy` fork.
    Dotenvy,
}

impl Competitor {
    /// Every adapter in the competitor matrix.
    pub const ALL: [Self; 5] = [
        Self::Dotenv,
        Self::DotenvFlow,
        Self::DotenvParser,
        Self::Dotenvs,
        Self::Dotenvy,
    ];

    /// Returns the stable report name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dotenv => "dotenv",
            Self::DotenvFlow => "dotenv-flow",
            Self::DotenvParser => "dotenv-parser",
            Self::Dotenvs => "dotenvs",
            Self::Dotenvy => "dotenvy",
        }
    }

    /// Returns whether the adapter has an in-memory value parser.
    #[must_use]
    pub const fn supports_values(self) -> bool {
        matches!(self, Self::DotenvParser | Self::Dotenvs | Self::Dotenvy)
    }

    /// Returns whether the adapter has a file-loading API.
    #[must_use]
    pub const fn supports_load(self) -> bool {
        !matches!(self, Self::DotenvParser)
    }
}

/// A reason why a competitor has no comparable cell for a fixture or mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    /// The competitor name.
    pub competitor: &'static str,
    /// The unsupported mode, such as `values` or `load`.
    pub mode: &'static str,
    /// The fixture name.
    pub fixture: String,
    /// The reason the cell is excluded.
    pub reason: &'static str,
}

/// Parses a fixture in memory through a competitor.
///
/// # Errors
///
/// Returns an error for an unsupported adapter/API or when the adapter rejects
/// the fixture.
pub fn parse_values(
    competitor: Competitor,
    source: &str,
) -> Result<BTreeMap<String, String>, String> {
    match competitor {
        Competitor::DotenvParser => {
            dotenv_parser::parse_dotenv(source).map_err(|error| format!("dotenv-parser: {error}"))
        }
        Competitor::Dotenvs => {
            let parsed = dotenvs::from_read(Cursor::new(source.as_bytes()))
                .map_err(|error| error.to_string())?;
            Ok(parsed
                .iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect())
        }
        Competitor::Dotenvy => dotenvy::from_read_iter(Cursor::new(source.as_bytes()))
            .map(|result| result.map_err(|error| error.to_string()))
            .collect::<Result<BTreeMap<_, _>, _>>(),
        Competitor::Dotenv | Competitor::DotenvFlow => Err(format!(
            "{} has no in-memory values API",
            competitor.label()
        )),
    }
}

/// Loads a fixture from a file through a competitor.
///
/// # Errors
///
/// Returns an error for an unsupported adapter/API or when the adapter rejects
/// the fixture.
#[allow(deprecated)]
pub fn load_values(
    competitor: Competitor,
    path: &Path,
) -> Result<BTreeMap<String, String>, String> {
    match competitor {
        Competitor::Dotenv => dotenv::from_path_iter(path)
            .map_err(|error| error.to_string())?
            .map(|result| result.map_err(|error| error.to_string()))
            .collect::<Result<BTreeMap<_, _>, _>>(),
        Competitor::DotenvFlow => dotenv_flow::from_path_iter(path)
            .map_err(|error| error.to_string())?
            .map(|result| result.map_err(|error| error.to_string()))
            .collect::<Result<BTreeMap<_, _>, _>>(),
        Competitor::Dotenvs => {
            let parsed = dotenvs::from_path(path).map_err(|error| error.to_string())?;
            Ok(parsed
                .iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect())
        }
        Competitor::Dotenvy => dotenvy::from_path_iter(path)
            .map_err(|error| error.to_string())?
            .map(|result| result.map_err(|error| error.to_string()))
            .collect::<Result<BTreeMap<_, _>, _>>(),
        Competitor::DotenvParser => Err("dotenv-parser has no file-loading API".to_owned()),
    }
}

/// Loads the same source through adotenv.
///
/// This is the normalized expectation used to validate competitor output.
///
/// # Errors
///
/// Returns an error when the source cannot be loaded with the portable dialect.
pub fn adotenv_load_values(source: &str) -> Result<BTreeMap<String, String>, String> {
    let path = temporary_path();
    fs::write(&path, source).map_err(|error| error.to_string())?;
    let result = adotenv::load::load_values([path.as_path()], adotenv::Dialect::PORTABLE)
        .map(adotenv::load::LoadedValues::into_values)
        .map_err(|error| error.to_string());
    let _ = fs::remove_file(path);
    result
}

/// Returns the pin/version string for a competitor.
#[must_use]
pub const fn version(competitor: Competitor) -> &'static str {
    match competitor {
        Competitor::Dotenv => "0.15.0",
        Competitor::DotenvFlow => "0.16.2",
        Competitor::DotenvParser => "0.1.3",
        Competitor::Dotenvs => "0.2.2",
        Competitor::Dotenvy => "0.15.7",
    }
}

/// Returns every unsupported competitor/mode/fixture cell and its reason.
#[must_use]
pub fn unsupported_cells() -> Vec<Unsupported> {
    let mut cells = Vec::new();
    for size in crate::Size::ALL {
        for shape in Shape::ALL {
            let fixture = format!("{}-{}", shape.label(), size.label());
            for competitor in Competitor::ALL {
                if !COMPARABLE_VALUES_SHAPES.contains(&shape) {
                    cells.push(Unsupported {
                        competitor: competitor.label(),
                        mode: "values",
                        fixture: fixture.clone(),
                        reason: "fixture semantics differ or are not validated for value parsing",
                    });
                } else if !competitor.supports_values() {
                    cells.push(Unsupported {
                        competitor: competitor.label(),
                        mode: "values",
                        fixture: fixture.clone(),
                        reason: "adapter has no in-memory values API",
                    });
                }
                if !COMPARABLE_LOAD_SHAPES.contains(&shape) {
                    cells.push(Unsupported {
                        competitor: competitor.label(),
                        mode: "load",
                        fixture: fixture.clone(),
                        reason: "fixture semantics differ or are not validated for loading",
                    });
                } else if !competitor.supports_load() {
                    cells.push(Unsupported {
                        competitor: competitor.label(),
                        mode: "load",
                        fixture: fixture.clone(),
                        reason: "adapter has no file-loading API",
                    });
                }
                cells.push(Unsupported {
                    competitor: competitor.label(),
                    mode: "resolve",
                    fixture: fixture.clone(),
                    reason: RESOLVE_UNSUPPORTED_REASON,
                });
            }
        }
    }
    cells
}

fn temporary_path() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "adotenv-bench-{}-{}.env",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixture, Shape, Size};
    use adotenv::parse_values as parse_adotenv_values;

    fn fixture_source(shape: Shape) -> String {
        fixture(Size::Eight, shape).source().to_owned()
    }

    #[test]
    fn in_memory_adapters_validate_supported_shapes() {
        for shape in COMPARABLE_VALUES_SHAPES {
            let source = fixture_source(shape);
            let expected = parse_adotenv_values(&source).expect("fixture is valid Portable syntax");
            for competitor in Competitor::ALL.into_iter().filter(|c| c.supports_values()) {
                let actual = super::parse_values(competitor, &source)
                    .unwrap_or_else(|error| panic!("{} {shape:?}: {error}", competitor.label()));
                assert_eq!(actual, expected, "{} {shape:?}", competitor.label());
            }
        }
    }

    #[test]
    fn file_adapters_validate_supported_shapes() {
        for shape in COMPARABLE_LOAD_SHAPES {
            let source = fixture_source(shape);
            let expected = adotenv_load_values(&source).expect("fixture loads");
            let path = temporary_path();
            fs::write(&path, &source).expect("write fixture");
            for competitor in Competitor::ALL.into_iter().filter(|c| c.supports_load()) {
                let actual = load_values(competitor, &path)
                    .unwrap_or_else(|error| panic!("{} {shape:?}: {error}", competitor.label()));
                assert_eq!(actual, expected, "{} {shape:?}", competitor.label());
            }
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn pins_are_recorded() {
        for competitor in Competitor::ALL {
            assert_ne!(version(competitor), "");
        }
    }

    #[test]
    fn unsupported_cells_are_explicit() {
        let cells = unsupported_cells();
        assert!(cells.iter().any(|cell| cell.mode == "values"));
        assert!(cells.iter().any(|cell| cell.mode == "load"));
        assert!(cells.iter().any(|cell| cell.competitor == "dotenv"));
    }
}
