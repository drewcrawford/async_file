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

## Design Philosophy

This library enforces that only one operation may be in-flight at a time per file handle.
This constraint simplifies the implementation and prevents many classes of concurrency bugs.

The library uses opaque types (`File`, `Data`, `Metadata`) that wrap platform-specific
implementations, providing a clean abstraction layer while maintaining efficiency.

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

### Utility Functions

```rust
// Check if a file exists
let exists = async_file::exists("/path/to/file", Priority::unit_test()).await;
```

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