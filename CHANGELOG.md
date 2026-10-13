# Changelog

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-13

First release, extracted from [alint](https://github.com/asamarts/alint)'s
`.env` support.

### Added

- `parse`: literal `.env` parsing into a `Document` of entries with line
  numbers and byte spans for each line, key and value.
- `encode_value`: write a value as `.env` syntax that parses back exactly.
- `set` and `remove`: edit one assignment in place, keeping every other byte,
  and refuse a missing or duplicated key.
