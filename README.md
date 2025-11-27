# async_file

Asynchronous file I/O operations with priority handling.

![logo](art/logo.png)

`async_file` provides a simple yet powerful API for performing asynchronous file operations
in Rust. It closely follows the standard library's file API design while adding async
support and priority-based scheduling.

## Features

* **Async Operations**: All file operations are asynchronous, allowing for non-blocking I/O
* **Priority Scheduling**: Every operation accepts a priority parameter for fine-grained control
* **Memory Safety**: Uses an opaque `Data` type to safely handle OS-managed memory allocations
* **Platform Agnostic**: Backend-agnostic API with a default std implementation

## Quick Start

```rust
use async_file::{File, Priority};

// Open a file with unit test priority
let file = File::open("/dev/zero", Priority::unit_test()).await?;

// Read up to 1KB of data
let data = file.read(1024, Priority::unit_test()).await?;
println!("Read {} bytes", data.len());
```

## Architecture Overview

### Opaque Type Design

The library uses opaque wrapper types that hide platform-specific implementations:

- `File`: Wraps platform file handles behind a unified async interface
- `Data`: Encapsulates OS-managed memory buffers for safe async I/O
- `Metadata`: Provides file information in a platform-agnostic way
- `Error`: Wraps platform-specific error types

This design ensures API stability while allowing platform-specific optimizations.

### Single Operation Constraint

**Important**: Only one operation may be in-flight at a time per file handle.

This constraint:
- Prevents race conditions on file position
- Simplifies the implementation
- Avoids many classes of concurrency bugs
- Matches typical file I/O patterns

Attempting concurrent operations on the same file handle will result in undefined behavior.

### Memory Management Strategy

The library uses an opaque `Data` type instead of user-provided buffers. This design:

- **Prevents use-after-free bugs**: If an async operation is cancelled (by dropping the
  future), the OS might still write to the buffer. OS-managed allocation prevents this.
- **Enables platform optimizations**: Different platforms can use their optimal memory
  allocation strategies.
- **Simplifies the API**: Users don't need to manage buffer lifetimes across await points.

## Common Usage Patterns

### Reading a File Completely

```rust
use async_file::{File, Priority};

// For small files, use read_all()
let file = File::open("config.txt", Priority::highest_async()).await?;
let contents = file.read_all(Priority::highest_async()).await?;

// Convert to String if needed
let text = String::from_utf8(contents.into_boxed_slice().into_vec())
    .expect("Invalid UTF-8");
```

### Sequential Reading with Seeking

```rust
use async_file::{File, Priority};
use std::io::SeekFrom;

let mut file = File::open("/dev/zero", Priority::unit_test()).await?;

// Read header (first 128 bytes)
let header = file.read(128, Priority::unit_test()).await?;

// Skip to data section at byte 1024
file.seek(SeekFrom::Start(1024), Priority::unit_test()).await?;

// Read data
let data = file.read(4096, Priority::unit_test()).await?;
```

### Checking File Existence Before Opening

```rust
use async_file::{exists, File, Priority};

let path = "important.dat";

if exists(path, Priority::unit_test()).await {
    let file = File::open(path, Priority::highest_async()).await?;
    // Process file...
} else {
    eprintln!("File not found: {}", path);
}
```

### Priority-Based Operations

```rust
use async_file::{File, Priority};

// Critical system file - use highest priority
let system_file = File::open("/critical/system.conf",
    Priority::highest_async()).await?;

// Background logging - use low priority
// For other priority levels, use Priority::new()
// Priority::new(0.2) for low priority tasks

// User-facing operation - use high priority
// Priority::new(0.8) for high priority tasks

// Unit tests - use dedicated test priority
let test_file = File::open("test_fixture.txt",
    Priority::unit_test()).await?;
```

## API Overview

### File Operations

```rust
use async_file::{File, Priority};
use std::io::SeekFrom;

// Open a file
let mut file = File::open("/path/to/file", Priority::unit_test()).await?;

// Read data
let data = file.read(1024, Priority::unit_test()).await?;

// Seek to position
let pos = file.seek(SeekFrom::Start(100), Priority::unit_test()).await?;

// Get metadata
let metadata = file.metadata(Priority::unit_test()).await?;
println!("File size: {} bytes", metadata.len());

// Read entire file
let contents = file.read_all(Priority::unit_test()).await?;
```

### Memory Management

The `Data` type provides safe access to OS-managed memory:

```rust
let data = file.read(100, Priority::unit_test()).await?;

// Access as a slice
let bytes: &[u8] = data.as_ref();

// Convert to owned data (may require copying)
let boxed: Box<[u8]> = data.into_boxed_slice();
```

### Platform Support

- **Unix/Linux/macOS**: Uses `blocking` crate to run `std::fs` operations in a thread pool
- **WASM**: Uses web fetch API for remote file access (requires `set_default_origin`)
- **Windows**: Same as Unix implementation using `blocking` crate

### Utility Functions

#### Checking File Existence

```rust
use async_file::{exists, Priority};

// Check if a file exists
if exists("/path/to/file", Priority::unit_test()).await {
    println!("File exists");
} else {
    println!("File not found");
}
```

#### Setting Default Origin for WASM

In WASM environments, files are fetched from remote URLs rather than accessed from a local filesystem. Use `set_default_origin` to set the base URL for these fetch operations.

```rust
use async_file::set_default_origin;

// Set origin for WASM file fetching
set_default_origin("https://cdn.example.com/assets");

// On non-WASM platforms, this is a no-op
```

**When to use**: Call this function at application startup when running in WASM environments (particularly Node.js) where the origin URL cannot be determined automatically, or when you need to fetch files from a specific server.

## Priority System

All operations require a priority parameter from the `priority` crate for scheduling control:

```rust
use async_file::Priority;

// Different priority levels
let high_priority = Priority::highest_async();
let test_priority = Priority::unit_test();
```

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE.md) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT.md) or http://opensource.org/licenses/MIT)

at your option.