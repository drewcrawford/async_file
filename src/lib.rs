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

# Design Philosophy

This library enforces that only one operation may be in-flight at a time per file handle.
This constraint simplifies the implementation and prevents many classes of concurrency bugs.

The library uses opaque types (`File`, `Data`, `Metadata`) that wrap platform-specific
implementations, providing a clean abstraction layer while maintaining efficiency.
*/

mod std_impl;

use std::hash::Hash;
use std::path::Path;
use std_impl as sys;

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

impl Into<Box<[u8]>> for Data {
    fn into(self) -> Box<[u8]> {
        self.into_boxed_slice()
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
    /// ```no_run
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

/**
Tests if a file or directory exists at the given path.

# Examples

```rust
# async fn example() {
use async_file::{exists, Priority};

let exists = exists("/dev/zero", Priority::unit_test()).await;
if exists {
    println!("File exists");
}
# }
```
*/
pub async fn exists(path: impl AsRef<Path>, priority: Priority) -> bool {
    sys::exists(path, priority).await
}

/// An error that can occur during file operations.
///
/// This is a wrapper around platform-specific error types. It implements
/// `std::error::Error` and can be converted to/from `std::io::Error`.
///
/// # Examples
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
#[derive(Debug)]
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
}

/*
boilerplates.

Data - OS probably supports a clone op via refcount, but i think we don't want to expose it – use rc/arc if you want that.
PartialEq and Eq are at least possible to implement via slice
Ord does not make a ton of sense to me

Hash is possible...
No to default/display
Send/sync ought to be possible, since it's immutable
unpin - should be safe to unpin, even if it seems to have an internal pointer somewhere.
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
File
fs does not have a clone op, so we don't either
does not have eq or ord, hash, default, display

No asref/asmut, as we don't want to expose internals

Files ought to be send at least.  Probably sync as well, although we don't expose many immutable methods.
I think we don't expect the OS to have pointers into them, so unpin should be safe.
 */

/*
Metadata
std derives Clone but not Copy
Doesn't look like we support Eq, Ord, Hash, etc.
We do have send/sync and Unpin
 */

#[cfg(test)]
mod tests {
    use crate::{Data, File, Metadata, Priority};

    #[test]
    fn test_open_file() {
        logwise::context::Context::reset("test_open_file");
        test_executors::spin_on(async {
            let _file = File::open("/dev/zero", Priority::unit_test())
                .await
                .unwrap();
        });
    }
    #[test]
    fn test_read_file() {
        logwise::context::Context::reset("test_read_file");
        test_executors::spin_on(async {
            let file = File::open("/dev/zero", Priority::unit_test())
                .await
                .unwrap();
            let buf = file.read(1024, Priority::unit_test()).await.unwrap();
            assert_eq!(buf.len(), 1024);
            assert_eq!(buf.iter().all(|&x| x == 0), true);
        });
    }

    #[test]
    fn test_seek_file() {
        logwise::context::Context::reset("test_seek_file");
        test_executors::spin_on(async {
            let mut file = File::open("/dev/zero", Priority::unit_test())
                .await
                .unwrap();
            let pos = file
                .seek(std::io::SeekFrom::Start(1024), Priority::unit_test())
                .await
                .unwrap();
            assert_eq!(pos, 1024);
        });
    }

    #[test]
    fn test_send_sync() {
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<Data>();
        _assert_send_sync::<File>();
        _assert_send_sync::<Metadata>();
    }

    #[test]
    fn test_unpin() {
        fn _assert_unpin<T: Unpin>() {}
        _assert_unpin::<Data>();
        _assert_unpin::<File>();
        _assert_unpin::<Metadata>();
    }

    #[test]
    fn test_length() {
        logwise::context::Context::reset("test_length");
        test_executors::spin_on(async {
            let file = File::open("/dev/zero", Priority::unit_test())
                .await
                .unwrap();
            let metadata = file.metadata(Priority::unit_test()).await.unwrap();
            assert_eq!(metadata.len(), 0);
        });
    }

    #[test]
    fn test_exists() {
        logwise::context::Context::reset("test_exists");
        test_executors::spin_on(async {
            assert_eq!(
                crate::exists("/dev/zero", Priority::unit_test()).await,
                true
            );
            assert_eq!(
                crate::exists("/nonexistent/path", Priority::unit_test()).await,
                false
            );
        });
    }
}
