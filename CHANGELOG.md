# Changelog

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `EditError::WouldChangeMeaning`: refuse an edit that would invalidate or
  reinterpret the remaining assignments.

### Fixed

- Verify removals by re-parsing and comparing the remaining ordered
  assignments and BOM state. Deleting the first line can no longer expose an
  embedded BOM and silently rename or drop another key.
- Build `set`'s double-quoted fallback only when the first encoding fails
  contextual readback, without repeating an already quoted candidate.
- Clarify that entry spans and `encode_value` alone do not guarantee a safe
  contextual edit; use the verified `set` and `remove` operations.
- Describe encoder failures without claiming that every `.env` reader rejects
  the value, and explicitly document 0.1's permissive quoted-tail behavior.

## [0.1.0] - 2026-10-13

First release, extracted from [alint](https://github.com/asamarts/alint)'s
`.env` support.

### Added

- `parse`: literal `.env` parsing into a `Document` of entries with line
  numbers and byte spans for each line, key and value.
- `encode_value`: write a value as `.env` syntax that parses back exactly.
- `set` and `remove`: edit one assignment in place, keeping every other byte,
  and refuse a missing or duplicated key.
