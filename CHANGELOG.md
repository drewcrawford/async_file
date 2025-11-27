# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- Updated logwise dependency to 0.4.0—keeping our logging game fresh
- Tidied up the CI configuration with the latest v9 syntax

### Fixed
- Error type now properly implements Unpin (consistency is key!)

### Internal
- Added .gitignore to keep the repo tidy
- Integrated standard build scripts for a smoother development experience

## [0.1.1] - 2025-06-08

### Added
- File existence checking support
- Improved documentation to help you get started

## [0.1.0] - 2024-10-23

### Added
- Initial release with core async file operations
- Support for reading files with configurable priorities
- Metadata queries for file information
- Convenience functions for common operations
- Send, Sync, and Unpin traits for all public types

[Unreleased]: https://github.com/drewcrawford/async_file/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/drewcrawford/async_file/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/drewcrawford/async_file/releases/tag/v0.1.0
