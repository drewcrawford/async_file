//SPDX-License-Identifier: MIT OR Apache-2.0
/*!
Asynchronous file I/O operations with priority handling.

![logo](../../../art/logo.png)

`async_file` provides a simple yet powerful API for performing asynchronous file operations
in Rust. It closely follows the standard library's file API design while adding async
support and priority-based scheduling.

# Features

* **Async Operations**: All file operations are asynchronous, allowing for non-blocking I/O
* **Priority Scheduling**: Every operation accepts a priority parameter for fine-grained control
* **Memory Safety**: Uses an opaque `Data` type to safely handle OS-managed memory allocations
* **Platform Agnostic**: Backend-agnostic API with a default std implementation

# Quick Start

```
# async fn example() -> Result<(), async_file::Error> {
use async_file::{File, Priority};

// Open a file with unit test priority
let file = File::open("/dev/zero", Priority::unit_test()).await?;

// Read up to 1KB of data
let data = file.read(1024, Priority::unit_test()).await?;
println!("Read {} bytes", data.len());
# Ok(())
# }
```

# Architecture Overview

## Opaque Type Design

The library uses opaque wrapper types that hide platform-specific implementations:

- [`File`]: Wraps platform file handles behind a unified async interface
- [`Data`]: Encapsulates OS-managed memory buffers for safe async I/O
- [`Metadata`]: Provides file information in a platform-agnostic way
- [`Error`]: Wraps platform-specific error types

This design ensures API stability while allowing platform-specific optimizations.

## Single Operation Constraint

**Important**: Only one operation may be in-flight at a time per file handle.

This constraint:
- Prevents race conditions on file position
- Simplifies the implementation
- Avoids many classes of concurrency bugs
- Matches typical file I/O patterns

Attempting concurrent operations on the same file handle will result in undefined behavior.

## Memory Management Strategy

The library uses an opaque [`Data`] type instead of user-provided buffers. This design:

- **Prevents use-after-free bugs**: If an async operation is cancelled (by dropping the
  future), the OS might still write to the buffer. OS-managed allocation prevents this.
- **Enables platform optimizations**: Different platforms can use their optimal memory
  allocation strategies.
- **Simplifies the API**: Users don't need to manage buffer lifetimes across await points.

# Common Usage Patterns

## Reading a File Completely

```
# async fn example() -> Result<(), async_file::Error> {
use async_file::{File, Priority};

// For small files, use read_all()
let file = File::open("config.txt", Priority::highest_async()).await?;
let contents = file.read_all(Priority::highest_async()).await?;

// Convert to String if needed
let text = String::from_utf8(contents.into_boxed_slice().into_vec())
    .expect("Invalid UTF-8");
# Ok(())
# }
```

## Sequential Reading with Seeking

```
# async fn example() -> Result<(), async_file::Error> {
use async_file::{File, Priority};
use std::io::SeekFrom;

let mut file = File::open("/dev/zero", Priority::unit_test()).await?;

// Read header (first 128 bytes)
let header = file.read(128, Priority::unit_test()).await?;

// Skip to data section at byte 1024
file.seek(SeekFrom::Start(1024), Priority::unit_test()).await?;

// Read data
let data = file.read(4096, Priority::unit_test()).await?;
# Ok(())
# }
```

## Checking File Existence Before Opening

```
# async fn example() -> Result<(), async_file::Error> {
use async_file::{exists, File, Priority};

let path = "important.dat";

if exists(path, Priority::unit_test()).await {
    let file = File::open(path, Priority::highest_async()).await?;
    // Process file...
} else {
    eprintln!("File not found: {}", path);
}
# Ok(())
# }
```

## Priority-Based Operations

```
# async fn example() -> Result<(), async_file::Error> {
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
# Ok(())
# }
```

# Platform Support

- **Unix/Linux/macOS**: Uses `blocking` crate to run `std::fs` operations in a thread pool
- **WASM**: Uses web fetch API for remote file access (requires `set_default_origin`)
- **Windows**: Same as Unix implementation using `blocking` crate

# Design Philosophy

This library enforces that only one operation may be in-flight at a time per file handle.
This constraint simplifies the implementation and prevents many classes of concurrency bugs.

The library uses opaque types (`File`, `Data`, `Metadata`) that wrap platform-specific
implementations, providing a clean abstraction layer while maintaining efficiency.
*/

/// Sets the default origin for WASM file operations.
///
/// In WASM environments, files are fetched from remote URLs rather than accessed
/// from a local filesystem. This function sets the base URL for these fetch operations.
///
/// # Platform Behavior
///
/// - **WASM**: Sets the base URL for fetching files
/// - **Other platforms**: No-op, provided for cross-platform compatibility
///
/// # When to Use
///
/// Call this function at application startup when:
/// - Running in WASM environments (particularly Node.js)
/// - The origin URL cannot be determined automatically
/// - You need to fetch files from a specific server
///
/// # Arguments
///
/// * `origin` - The base URL to use for file operations (e.g., `"https://example.com/files"`)
///
/// # Examples
///
/// ```
/// use async_file::set_default_origin;
///
/// // Set origin for WASM file fetching
/// set_default_origin("https://cdn.example.com/assets");
///
/// // On non-WASM platforms, this is a no-op
/// ```
///
/// # Note
///
/// This function uses a `'static` string to avoid lifetime issues with async operations.
/// The origin must be set before any file operations are performed.
pub fn set_default_origin(origin: &'static str) {
    sys::set_default_origin(origin);
}

#[cfg(not(target_arch = "wasm32"))]
mod std_impl;
#[cfg(target_arch = "wasm32")]
mod wasm_impl;

use std::hash::Hash;
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
use std_impl as sys;
#[cfg(target_arch = "wasm32")]
use wasm_impl as sys;

/// A handle to an open file for asynchronous I/O operations.
///
/// `File` provides async methods for reading, seeking, and retrieving metadata.
/// All operations require a priority parameter for scheduling control.
///
/// # Constraints
///
/// Only one operation may be in-flight at a time per file handle. This means
/// you cannot start a new operation until the previous one completes.
///
/// # Examples
///
/// ```
/// # async fn example() -> Result<(), async_file::Error> {
/// use async_file::{File, Priority};
///
/// let file = File::open("/dev/zero", Priority::unit_test()).await?;
/// let data = file.read(100, Priority::unit_test()).await?;
/// assert_eq!(data.len(), 100);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct File(sys::File);

/// A priority value for scheduling file operations.
///
/// This is a re-export of the `priority::Priority` type. Use this to control
/// the scheduling priority of your file operations.
pub type Priority = priority::Priority;

/// An opaque buffer type that holds data read from files.
///
/// `Data` represents memory that may be allocated and managed by the OS. It provides
/// safe access to the underlying bytes while ensuring proper cleanup through its
/// custom `Drop` implementation.
///
/// # Design Rationale
///
/// When performing async I/O, the OS may continue writing to a buffer even after
/// a Rust future is cancelled. By having the OS control both allocation and deallocation,
/// we avoid use-after-free bugs. This type safely wraps OS-managed memory.
///
/// # Usage
///
/// You can access the underlying bytes through several methods:
/// - `as_ref()` or `deref()` to get a `&[u8]` slice
/// - `into_boxed_slice()` to convert to a `Box<[u8]>` (may require copying)
///
/// # Examples
///
/// ```
/// # async fn example() -> Result<(), async_file::Error> {
/// use async_file::{File, Priority};
///
/// let file = File::open("/dev/zero", Priority::unit_test()).await?;
/// let data = file.read(10, Priority::unit_test()).await?;
///
/// // Access as a slice
/// assert_eq!(data.as_ref(), &[0; 10]);
///
/// // Or convert to a boxed slice
/// let boxed: Box<[u8]> = data.into_boxed_slice();
/// assert_eq!(&*boxed, &[0; 10]);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Data(sys::Data);

impl AsRef<[u8]> for Data {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl std::ops::Deref for Data {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.0.deref()
    }
}

impl Data {
    /// Converts this `Data` into a boxed byte slice.
    ///
    /// # Performance
    ///
    /// - On platforms where the underlying memory layout is compatible, this is
    ///   a zero-cost operation
    /// - On other platforms, this may require copying the data
    ///
    /// # When to Use
    ///
    /// Use this method when you need to:
    /// - Store the data in a collection that requires owned slices
    /// - Pass ownership to code expecting `Box<[u8]>`
    /// - Ensure the data outlives the original `Data` object
    ///
    /// If you only need to read the data, prefer using `as_ref()` or `deref()`
    /// to avoid potential copying.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::{File, Priority};
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let data = file.read(5, Priority::unit_test()).await?;
    ///
    /// // Convert to boxed slice for storage
    /// let boxed: Box<[u8]> = data.into_boxed_slice();
    /// assert_eq!(boxed.len(), 5);
    /// assert!(boxed.iter().all(|&b| b == 0));
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_boxed_slice(self) -> Box<[u8]> {
        self.0.into_boxed_slice()
    }
}

impl From<Data> for Box<[u8]> {
    fn from(val: Data) -> Self {
        val.into_boxed_slice()
    }
}

impl File {
    /// Opens a file at the given path for reading.
    ///
    /// This is an async operation that returns a `File` handle on success.
    /// The file is opened in read-only mode.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to open
    /// * `priority` - The priority for this operation
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file doesn't exist
    /// - Permissions are insufficient
    /// - Other I/O errors occur
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::{File, Priority};
    ///
    /// // Open a file with unit test priority
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    ///
    /// // Open with highest async priority for critical operations
    /// let important_file = File::open("/dev/zero", Priority::highest_async()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open(path: impl AsRef<Path>, priority: Priority) -> Result<Self, Error> {
        sys::File::open(path, priority)
            .await
            .map(File)
            .map_err(Error)
    }
    /// Reads up to `buf_size` bytes from the file.
    ///
    /// This method is similar to `std::fs::File::read` but with key differences:
    ///
    /// # Memory Management
    ///
    /// Unlike the standard library, memory is allocated by the OS rather than
    /// by the caller. This prevents use-after-free bugs if a read operation
    /// is cancelled (by dropping the future) while the OS is still writing
    /// to the buffer.
    ///
    /// # Return Value
    ///
    /// Returns a `Data` object containing the bytes read. The actual number
    /// of bytes read may be less than `buf_size` if:
    /// - End of file is reached
    /// - The read is interrupted
    ///
    /// # Constraints
    ///
    /// Only one operation may be in-flight at a time per file handle.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::{File, Priority};
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    ///
    /// // Read up to 1KB
    /// let data = file.read(1024, Priority::unit_test()).await?;
    /// println!("Read {} bytes", data.len());
    ///
    /// // Access the data as a slice
    /// let first_ten: &[u8] = &data[..10.min(data.len())];
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read(&self, buf_size: usize, priority: Priority) -> Result<Data, Error> {
        self.0
            .read(buf_size, priority)
            .await
            .map(Data)
            .map_err(Error)
    }

    /// Seeks to a position in the file.
    ///
    /// This method changes the position for the next read operation.
    /// It behaves like `std::fs::File::seek`.
    ///
    /// # Arguments
    ///
    /// * `pos` - The position to seek to, using `std::io::SeekFrom`
    /// * `priority` - The priority for this operation
    ///
    /// # Returns
    ///
    /// Returns the new position from the start of the file in bytes.
    ///
    /// # Constraints
    ///
    /// Only one operation may be in-flight at a time per file handle.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::{File, Priority};
    /// use std::io::SeekFrom;
    ///
    /// let mut file = File::open("/dev/zero", Priority::unit_test()).await?;
    ///
    /// // Seek to byte 100
    /// let pos = file.seek(SeekFrom::Start(100), Priority::unit_test()).await?;
    /// assert_eq!(pos, 100);
    ///
    /// // Seek forward 50 bytes from current position
    /// let new_pos = file.seek(SeekFrom::Current(50), Priority::unit_test()).await?;
    /// assert_eq!(new_pos, 150);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn seek(&mut self, pos: std::io::SeekFrom, priority: Priority) -> Result<u64, Error> {
        self.0.seek(pos, priority).await.map_err(Error)
    }

    /// Returns metadata about the file.
    ///
    /// This method retrieves information about the file such as its size.
    /// It behaves like `std::fs::File::metadata`.
    ///
    /// # Arguments
    ///
    /// * `priority` - The priority for this operation
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::{File, Priority};
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let metadata = file.metadata(Priority::unit_test()).await?;
    ///
    /// println!("File size: {} bytes", metadata.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn metadata(&self, priority: Priority) -> Result<Metadata, Error> {
        self.0.metadata(priority).await.map(Metadata).map_err(Error)
    }

    /// Reads the entire contents of the file.
    ///
    /// This is a convenience method that first retrieves the file's metadata
    /// to determine its size, then reads that many bytes.
    ///
    /// # Arguments
    ///
    /// * `priority` - The priority for this operation
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The metadata operation fails
    /// - The read operation fails
    /// - The file is too large to fit in memory
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::{File, Priority};
    ///
    /// let file = File::open("small_file.txt", Priority::unit_test()).await?;
    /// let contents = file.read_all(Priority::unit_test()).await?;
    ///
    /// println!("File contains {} bytes", contents.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_all(&self, priority: Priority) -> Result<Data, Error> {
        let metadata = self.0.metadata(priority).await.map(Metadata)?;
        let len = metadata.len();
        self.read(len.try_into().unwrap(), priority).await
    }
}

/// Tests if a file or directory exists at the given path.
///
/// This function checks whether a file or directory exists without opening it.
/// It's useful for conditional logic based on file presence.
///
/// # Arguments
///
/// * `path` - The path to check for existence
/// * `priority` - The priority for this operation
///
/// # Returns
///
/// - `true` if the file or directory exists
/// - `false` if it doesn't exist or if an error occurs checking
///
/// # Performance Note
///
/// This function performs a filesystem metadata query. For performance-critical
/// code, consider opening the file directly and handling the error instead of
/// checking existence first.
///
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// # async fn example() {
/// use async_file::{exists, Priority};
///
/// if exists("/dev/zero", Priority::unit_test()).await {
///     println!("File exists");
/// } else {
///     println!("File not found");
/// }
/// # }
/// ```
///
/// ## Conditional File Opening
///
/// ```
/// # async fn example() -> Result<(), async_file::Error> {
/// use async_file::{exists, File, Priority};
///
/// let path = "optional_config.json";
/// 
/// if exists(path, Priority::unit_test()).await {
///     let file = File::open(path, Priority::unit_test()).await?;
///     // Process configuration...
/// } else {
///     // Use defaults when config doesn't exist
///     println!("Using default configuration");
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Checking Multiple Files
///
/// ```
/// # async fn example() {
/// use async_file::{exists, Priority};
///
/// let required_files = ["data.db", "config.yml", "schema.sql"];
/// 
/// for file in &required_files {
///     if !exists(file, Priority::unit_test()).await {
///         eprintln!("Missing required file: {}", file);
///         return;
///     }
/// }
/// 
/// println!("All required files present");
/// # }
/// ```
pub async fn exists(path: impl AsRef<Path>, priority: Priority) -> bool {
    sys::exists(path, priority).await
}

/// An error that can occur during file operations.
///
/// This is a wrapper around platform-specific error types. It implements
/// `std::error::Error` and provides automatic conversion from the underlying
/// platform errors.
///
/// # Error Sources
///
/// Errors can originate from:
/// - File not found
/// - Permission denied
/// - I/O errors during read/write/seek operations
/// - Invalid file paths
/// - File system errors (disk full, etc.)
///
/// # Platform Differences
///
/// - **Standard platforms**: Wraps `std::io::Error`
/// - **WASM**: Wraps fetch API errors
///
/// # Error Handling
///
/// The error type implements `Display` and `Debug` for convenient error reporting.
/// It also implements `std::error::Error` for compatibility with error handling libraries.
///
/// # Examples
///
/// ## Basic Error Handling
///
/// ```
/// # async fn example() {
/// use async_file::{File, Priority};
///
/// match File::open("nonexistent.txt", Priority::unit_test()).await {
///     Ok(file) => println!("File opened successfully"),
///     Err(e) => eprintln!("Failed to open file: {}", e),
/// }
/// # }
/// ```
///
/// ## Using the ? Operator
///
/// ```
/// # async fn example() -> Result<(), async_file::Error> {
/// use async_file::{File, Priority};
///
/// // The ? operator works with async_file::Error
/// let file = File::open("data.txt", Priority::unit_test()).await?;
/// let contents = file.read_all(Priority::unit_test()).await?;
/// 
/// println!("Read {} bytes", contents.len());
/// # Ok(())
/// # }
/// ```
///
/// ## Converting to `Box<dyn Error>`
///
/// ```
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use async_file::{File, Priority};
///
/// // async_file::Error can be converted to Box<dyn Error>
/// let file = File::open("config.json", Priority::highest_async()).await?;
/// let data = file.read_all(Priority::highest_async()).await?;
/// 
/// # Ok(())
/// # }
/// ```
#[derive(Debug, thiserror::Error)]
#[error("afile error {0}")]
pub struct Error(#[from] sys::Error);

/// Metadata information about a file.
///
/// This structure contains file metadata such as size. It's returned by
/// the `File::metadata` method.
///
/// # Examples
///
/// ```
/// # async fn example() -> Result<(), async_file::Error> {
/// use async_file::{File, Priority};
///
/// let file = File::open("/dev/zero", Priority::unit_test()).await?;
/// let metadata = file.metadata(Priority::unit_test()).await?;
///
/// println!("File size: {} bytes", metadata.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Metadata(sys::Metadata);
impl Metadata {
    /// Returns the size of the file in bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::{File, Priority};
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let metadata = file.metadata(Priority::unit_test()).await?;
    ///
    /// // /dev/zero is a special file with size 0
    /// assert_eq!(metadata.len(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn len(&self) -> u64 {
        self.0.len()
    }

    /// Returns `true` if the file has a size of 0 bytes.
    ///
    /// This is a convenience method equivalent to `self.len() == 0`.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::{File, Priority};
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let metadata = file.metadata(Priority::unit_test()).await?;
    ///
    /// // /dev/zero is a special file that appears empty
    /// assert!(metadata.is_empty());
    /// assert_eq!(metadata.len(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

logwise::declare_logging_domain!();

/*
Boilerplate section, for types in order of appearance in the main section.
 */

/*
File

Clone: Not implemented. The underlying std::fs::File doesn't support Clone as it
represents an exclusive handle to an open file descriptor.

Copy: Not implemented for the same reason as Clone.

PartialEq/Eq: Not implemented. File handles are unique and equality comparison
doesn't make semantic sense.

Hash: Not implemented without Eq.

Default: Not implemented. There's no meaningful default file.

Display: Not implemented. File handles are not typically displayed to users.

AsRef/AsMut: Not implemented. We want to control the API surface and not expose
the underlying file handle directly.

Send/Sync: Automatically derived and safe. Files can be sent between threads
and accessed from multiple threads (though our API enforces single operation
at a time).

Unpin: Automatically derived and safe since there are no self-references.
 */

/*
Data

Clone: Not implemented. The OS probably supports a clone operation via refcount,
but we deliberately don't expose it. Use Arc<Data> if you need shared ownership.

Copy: Not implemented. Data represents potentially large buffers that shouldn't
be copied implicitly.

PartialEq/Eq: Implemented. Comparison via the underlying byte slice is meaningful
and useful for testing and validation.

Ord: Not implemented. Lexicographic ordering of byte data rarely makes sense
in file I/O contexts.

Hash: Implemented. Hashing byte data is useful for caching and deduplication
scenarios.

Default: Not implemented. There's no meaningful default data buffer.

Display: Not implemented. Binary data is not typically displayed as text.

Send/Sync: Automatically derived and safe since the data is immutable.

Unpin: Safe to unpin even if there are internal pointers, as the data is
immutable after creation.
 */

impl PartialEq for Data {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}
impl Eq for Data {}

impl Hash for Data {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

/*
Metadata

Clone: Implemented via derive. std::fs::Metadata supports Clone and it makes sense
for metadata to be cloneable as it's just informational data that users may want
to store or pass around.

Copy: Not implemented. std::fs::Metadata doesn't support Copy as it's not a trivial
type - it contains platform-specific metadata that may include complex structures.

PartialEq/Eq: Not implemented. std::fs::Metadata doesn't support equality comparison,
likely because metadata can include timestamps and other volatile information that
makes equality semantics unclear.

Hash: Not implemented. std::fs::Metadata doesn't support hashing, and without Eq
it wouldn't make sense anyway.

Default: Not implemented. There's no meaningful default metadata - metadata must
come from an actual file.

Display: Not implemented. Metadata is not typically formatted for end-user display.

From/Into: Not obvious conversions exist. We don't want users converting between
our metadata and std metadata directly.

AsRef/AsMut: Not implemented. We want to control the API surface and not expose
the underlying std::fs::Metadata directly.

Send/Sync: Automatically derived since std::fs::Metadata is Send + Sync.
Unpin: Automatically derived and safe since there are no self-references.
 */

#[cfg(test)]
mod tests {
    use crate::{set_default_origin, Data, File, Metadata, Priority};

    #[cfg(target_arch = "wasm32")]
    const TEST_FILE: &str = "5MB.zip";
    #[cfg(not(target_arch = "wasm32"))]
    const TEST_FILE: &str = "/dev/zero";

    #[cfg(target_arch = "wasm32")]
    const SEEK_FILE: &str = TEST_FILE;

    #[cfg(not(target_arch = "wasm32"))]
    const SEEK_FILE: &str = "/etc/services";

    #[test_executors::async_test]
    async fn test_open_file() {
        logwise::context::Context::reset("test_open_file".to_string());
        set_default_origin("http://ipv4.download.thinkbroadband.com/");
        let _file = File::open(TEST_FILE, Priority::unit_test())
            .await
            .unwrap();
    }
    #[test_executors::async_test]
    async fn test_read_file() {
        logwise::context::Context::reset("test_read_file".to_string());
        set_default_origin("http://ipv4.download.thinkbroadband.com/");
        let file = File::open(TEST_FILE, Priority::unit_test())
            .await
            .unwrap();
        let buf = file.read(1024, Priority::unit_test()).await.unwrap();
        assert_eq!(buf.len(), 1024);
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(buf.iter().all(|&x| x == 0), true);
        #[cfg(target_arch = "wasm32")]
        assert!(buf.starts_with(&[121, 153, 245, 9, 197, 194]), "Expected output, got: {:?}", buf);
    }

    #[test_executors::async_test]
    async fn test_seek_file() {
        logwise::context::Context::reset("test_seek_file".to_string());
        set_default_origin("http://ipv4.download.thinkbroadband.com/");

        //tough to seek /dev/zero on linux for some reason
        let mut file = File::open(SEEK_FILE, Priority::unit_test())
            .await
            .unwrap();
        let pos = file
            .seek(std::io::SeekFrom::Start(1024), Priority::unit_test())
            .await
            .unwrap();
        assert_eq!(pos, 1024);
        let buf = file.read(1024, Priority::unit_test()).await.unwrap();
        assert_eq!(buf.len(), 1024);

    }

    #[test]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn test_send_sync() {
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<Data>();
        _assert_send_sync::<File>();
        _assert_send_sync::<Metadata>();
    }

    #[test]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn test_unpin() {
        fn _assert_unpin<T: Unpin>() {}
        _assert_unpin::<Data>();
        _assert_unpin::<File>();
        _assert_unpin::<Metadata>();
    }

    #[test_executors::async_test]
    async fn test_length() {
        set_default_origin("http://ipv4.download.thinkbroadband.com/");

        logwise::context::Context::reset("test_length".to_string());
        let file = File::open(TEST_FILE, Priority::unit_test())
            .await
            .unwrap();
        let metadata = file.metadata(Priority::unit_test()).await.unwrap();
        #[cfg(target_arch = "wasm32")]
        assert_eq!(metadata.len(), 5242880);
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(metadata.len(), 0);
    }

    #[test_executors::async_test]
    async fn test_exists() {
        logwise::context::Context::reset("test_exists".to_string());
        set_default_origin("http://ipv4.download.thinkbroadband.com/");
        assert_eq!(
            crate::exists(TEST_FILE, Priority::unit_test()).await,
            true
        );
        assert_eq!(
            crate::exists("/nonexistent/path", Priority::unit_test()).await,
            false
        );
    }
}
