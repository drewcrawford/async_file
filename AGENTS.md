# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`async_file` is a Rust library providing asynchronous file I/O operations with priority handling. The library wraps platform-specific implementations behind a unified async interface.

## Architecture

### Platform Implementation Split
- **`src/lib.rs`**: Main public API with opaque types (`File`, `Data`, `Metadata`) that wrap platform implementations
- **`src/std_impl.rs`**: Standard library implementation using `blocking` crate to make sync I/O async (Unix/Linux/macOS/Windows)
- **`src/wasm_impl.rs`**: WebAssembly implementation using Fetch API to read files served over HTTP

### Key Design Constraints
- **Single Operation Constraint**: Only one file operation may be in-flight at a time per file handle
- **OS-Controlled Allocation**: The `Data` type allows OS to manage memory allocation, preventing use-after-free bugs if async operations are cancelled
- **Priority-Based Operations**: All async operations accept priority parameters from the `priority` crate for scheduling control

### Memory Safety Strategy
The library uses an opaque `Data` type instead of user-provided buffers. This prevents a critical bug: if an async read operation is cancelled (by dropping the future), the OS might still write to the buffer, causing use-after-free. By having the OS control allocation, this class of bugs is eliminated.

## Development Commands

```bash
# Run comprehensive CI-style checks (fmt, check, clippy, tests, docs for both native and wasm32)
./scripts/check_all

# Run all tests (native + wasm32)
./scripts/tests

# Run only native tests
./scripts/native/tests

# Run only wasm32 tests (requires nightly toolchain)
./scripts/wasm32/tests

# Run a specific test
cargo test test_open_file

# Check, clippy, docs (both platforms)
./scripts/check
./scripts/clippy
./scripts/docs

# Format check
./scripts/fmt

# Use --relaxed flag to disable -D warnings (useful during development)
./scripts/wasm32/tests --relaxed
```

## Platform-Specific Testing

### Standard Platforms (Unix/Linux/macOS/Windows)
- Tests use `/dev/zero` for reliable file operations
- Seek tests use `/etc/services` on Unix-like systems

### WASM Platform
- Tests use `5MB.zip` fetched from `http://ipv4.download.thinkbroadband.com/`
- Requires `set_default_origin()` to be called with the test server URL
- Uses `wasm-bindgen-test-runner` with nightly toolchain
- The `_env` script in `scripts/wasm32/` sets up required RUSTFLAGS from cargo config

## Dependencies

### Core Dependencies
- `blocking` (non-WASM): Converts sync operations to async using thread pool
- `thiserror`: Error handling with derive macros
- `logwise`: Performance logging and warnings (use `logwise::info_sync!` etc.)
- `priority`: Priority scheduling system

### WASM-Specific Dependencies
- `web-sys`: Browser API bindings for Fetch, Request, Response
- `wasm-bindgen-futures`: JavaScript Promise to Rust Future conversion
- `some_executor`: WASM-compatible async executor

### Testing
- `test_executors`: Cross-platform async test runtime
- `wasm-bindgen-test`: WASM test harness

## Testing Notes

- Each test calls `logwise::context::Context::reset()` for proper logging isolation
- Use `test_executors::spin_on` for async test execution on standard platforms
- WASM tests use `#[wasm_bindgen_test]` attribute
- The constant `TEST_FILE` switches between `/dev/zero` (standard) and `5MB.zip` (WASM)