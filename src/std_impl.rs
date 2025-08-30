// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standard library implementation of async file I/O operations.
//!
//! This module provides the default implementation of `async_file`'s file operations
//! for non-WASM platforms. It uses the `blocking` crate to convert synchronous
//! standard library file operations into asynchronous operations.
//!
//! # Architecture
//!
//! The implementation wraps `std::fs::File` in an `Arc` to enable safe cloning for
//! async operations. Each async operation:
//!
//! 1. Clones the `Arc<std::fs::File>` to get an owned handle
//! 2. Uses `blocking::unblock` to run the sync operation in a thread pool
//! 3. Returns the result wrapped in platform-agnostic types
//!
//! # Performance Considerations
//!
//! This implementation uses a thread pool for I/O operations, which may not be
//! optimal for all use cases. Each operation logs a performance warning via
//! `logwise::perfwarn_begin!` to inform developers that true async I/O is not
//! being used.
//!
//! # Types
//!
//! - [`File`]: Wraps `std::fs::File` with async methods
//! - [`Data`]: Wraps byte data read from files
//! - [`Metadata`]: Wraps file metadata information
//! - [`Error`]: Platform-specific error type
//!
//! # Examples
//!
//! ## Basic File Operations
//!
//! ```
//! # async fn example() -> Result<(), async_file::Error> {
//! use async_file::{File, Priority};
//!
//! // Open and read from a file
//! let file = File::open("/dev/zero", Priority::unit_test()).await?;
//! let data = file.read(1024, Priority::unit_test()).await?;
//! assert_eq!(data.len(), 1024);
//! # Ok(())
//! # }
//! # test_executors::spin_on(example()).unwrap();
//! ```
//!
//! ## Seeking and Reading
//!
//! ```
//! # async fn example() -> Result<(), async_file::Error> {
//! use async_file::{File, Priority};
//! use std::io::SeekFrom;
//!
//! let mut file = File::open("/dev/zero", Priority::unit_test()).await?;
//!
//! // Seek to position 100
//! file.seek(SeekFrom::Start(100), Priority::unit_test()).await?;
//!
//! // Read from the new position
//! let data = file.read(50, Priority::unit_test()).await?;
//! assert_eq!(data.len(), 50);
//! # Ok(())
//! # }
//! # test_executors::spin_on(example()).unwrap();
//! ```

use crate::Priority;
use blocking::unblock;
use std::io::Read;
use std::io::Seek;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

/// A file handle for asynchronous I/O operations.
///
/// This struct wraps a standard library `File` in an `Arc` to enable safe
/// cloning for async operations. The `Arc` allows multiple async operations
/// to hold references to the same underlying file descriptor.
///
/// # Implementation Details
///
/// The file is wrapped in an `Arc` rather than using `&self` references because:
/// - Async operations need to move the file handle into the blocking thread pool
/// - Multiple operations may need to hold references during their lifetime
/// - The `Arc` ensures the file stays alive until all operations complete
///
/// # Thread Safety
///
/// While the `Arc` allows multiple references, the main `async_file` API ensures
/// only one operation is in-flight at a time per file handle, preventing race
/// conditions on the file position.
///
/// # Example
///
/// ```
/// # use async_file::Priority;
/// # async fn example() -> Result<(), async_file::Error> {
/// // Internal usage - users interact through the main async_file::File type
/// use async_file::File;
///
/// let file = File::open("/dev/zero", Priority::unit_test()).await?;
/// let data = file.read(100, Priority::unit_test()).await?;
/// assert_eq!(data.len(), 100);
/// # Ok(())
/// # }
/// # test_executors::spin_on(example()).unwrap();
/// ```
#[derive(Debug)]
pub struct File(Arc<std::fs::File>);

/// Error type for file operations in the standard library implementation.
///
/// This enum wraps I/O errors from the standard library and provides
/// automatic conversion via the `thiserror` derive macro.
///
/// # Variants
///
/// - `Io`: Wraps a standard library I/O error
///
/// # Non-exhaustive
///
/// This enum is marked `#[non_exhaustive]` to allow adding new error
/// variants in future versions without breaking compatibility.
///
/// # Example
///
/// ```
/// # use async_file::Priority;
/// # async fn example() {
/// use async_file::File;
///
/// match File::open("nonexistent.txt", Priority::unit_test()).await {
///     Ok(_) => println!("File opened"),
///     Err(e) => eprintln!("Error: {}", e),
/// }
/// # }
/// # test_executors::spin_on(example());
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// A buffer containing data read from a file.
///
/// This struct wraps a boxed byte slice containing the data read from a file.
/// It provides safe access to the underlying bytes while ensuring proper
/// memory management.
///
/// # Memory Management
///
/// The data is stored as a `Box<[u8]>` which:
/// - Owns the memory allocation
/// - Automatically deallocates when dropped
/// - Can be efficiently converted to other owned types
///
/// # Access Patterns
///
/// The struct implements `AsRef<[u8]>` and `Deref` to provide convenient
/// access to the underlying bytes without requiring explicit method calls.
///
/// # Example
///
/// ```
/// # use async_file::Priority;
/// # async fn example() -> Result<(), async_file::Error> {
/// use async_file::File;
///
/// let file = File::open("/dev/zero", Priority::unit_test()).await?;
/// let data = file.read(10, Priority::unit_test()).await?;
///
/// // Access as a slice
/// assert_eq!(data.as_ref(), &[0; 10]);
///
/// // Direct indexing via Deref
/// assert_eq!(data[0], 0);
///
/// // Convert to owned Box<[u8]>
/// let boxed = data.into_boxed_slice();
/// assert_eq!(boxed.len(), 10);
/// # Ok(())
/// # }
/// # test_executors::spin_on(example()).unwrap();
/// ```
#[derive(Debug)]
pub struct Data(Box<[u8]>);

/// File metadata information.
///
/// This struct wraps the standard library's `Metadata` type, providing
/// information about a file such as its size.
///
/// # Cloning
///
/// `Metadata` implements `Clone` because the underlying `std::fs::Metadata`
/// is cloneable. This allows users to store or pass around metadata without
/// needing to re-query the file system.
///
/// # Available Information
///
/// Currently exposes:
/// - `len()`: The size of the file in bytes
///
/// Additional metadata fields from `std::fs::Metadata` could be exposed
/// in future versions as needed.
///
/// # Example
///
/// ```
/// # use async_file::Priority;
/// # async fn example() -> Result<(), async_file::Error> {
/// use async_file::File;
///
/// let file = File::open("/dev/zero", Priority::unit_test()).await?;
/// let metadata = file.metadata(Priority::unit_test()).await?;
///
/// // /dev/zero is a special file with size 0
/// assert_eq!(metadata.len(), 0);
///
/// // Metadata can be cloned
/// let metadata_copy = metadata.clone();
/// assert_eq!(metadata_copy.len(), 0);
/// # Ok(())
/// # }
/// # test_executors::spin_on(example()).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Metadata(std::fs::Metadata);

impl Metadata {
    /// Returns the size of the file in bytes.
    ///
    /// This method delegates to the underlying `std::fs::Metadata::len()`.
    ///
    /// # Example
    ///
    /// ```
    /// # use async_file::Priority;
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::File;
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let metadata = file.metadata(Priority::unit_test()).await?;
    /// println!("File size: {} bytes", metadata.len());
    /// # Ok(())
    /// # }
    /// # test_executors::spin_on(example()).unwrap();
    /// ```
    pub fn len(&self) -> u64 {
        self.0.len()
    }
}

impl AsRef<[u8]> for Data {
    /// Returns a reference to the underlying byte slice.
    ///
    /// # Example
    ///
    /// ```
    /// # use async_file::Priority;
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::File;
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let data = file.read(5, Priority::unit_test()).await?;
    ///
    /// // Use as_ref() to get a slice
    /// let slice: &[u8] = data.as_ref();
    /// assert_eq!(slice, &[0, 0, 0, 0, 0]);
    /// # Ok(())
    /// # }
    /// # test_executors::spin_on(example()).unwrap();
    /// ```
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Deref for Data {
    type Target = [u8];

    /// Dereferences to the underlying byte slice.
    ///
    /// This allows direct indexing and slice operations on `Data`.
    ///
    /// # Example
    ///
    /// ```
    /// # use async_file::Priority;
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::File;
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let data = file.read(10, Priority::unit_test()).await?;
    ///
    /// // Direct indexing via Deref
    /// assert_eq!(data[0], 0);
    /// assert_eq!(data[9], 0);
    ///
    /// // Slice operations
    /// let first_five = &data[..5];
    /// assert_eq!(first_five.len(), 5);
    /// # Ok(())
    /// # }
    /// # test_executors::spin_on(example()).unwrap();
    /// ```
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl Data {
    /// Converts this `Data` into a boxed byte slice.
    ///
    /// This method consumes the `Data` and returns the underlying `Box<[u8]>`.
    /// This is a zero-cost operation as it simply unwraps the inner value.
    ///
    /// # When to Use
    ///
    /// Use this method when you need:
    /// - To pass ownership to code expecting `Box<[u8]>`
    /// - To store the data in a collection
    /// - To avoid the `Data` wrapper type
    ///
    /// # Example
    ///
    /// ```
    /// # use async_file::Priority;
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::File;
    ///
    /// let file = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let data = file.read(8, Priority::unit_test()).await?;
    ///
    /// // Convert to Box<[u8]> for storage
    /// let boxed: Box<[u8]> = data.into_boxed_slice();
    /// assert_eq!(boxed.len(), 8);
    /// assert!(boxed.iter().all(|&b| b == 0));
    /// # Ok(())
    /// # }
    /// # test_executors::spin_on(example()).unwrap();
    /// ```
    pub fn into_boxed_slice(self) -> Box<[u8]> {
        self.0
    }
}

impl File {
    fn new(file: std::fs::File) -> Self {
        File(Arc::new(file))
    }
    pub async fn open(path: impl AsRef<Path>, _priority: Priority) -> Result<Self, Error> {
        logwise::perfwarn_begin!("async_file uses blocking on this platform");
        let path = path.as_ref().to_owned();
        unblock(|| std::fs::File::open(path))
            .await
            .map(File::new)
            .map_err(|e| e.into())
    }

    pub async fn read(&self, buf_size: usize, _priority: Priority) -> Result<Data, Error> {
        let mut move_file = self.0.clone();
        logwise::perfwarn_begin!("async_file uses blocking on this platform");
        unblock(move || {
            let mut buf = vec![0; buf_size];
            let read = move_file.read(&mut buf);
            match read {
                Ok(read) => {
                    buf.truncate(read);
                    Ok(buf.into_boxed_slice())
                }
                Err(e) => Err(e),
            }
        })
        .await
        .map(Data)
        .map_err(|e| e.into())
    }

    pub async fn seek(
        &mut self,
        pos: std::io::SeekFrom,
        _priority: Priority,
    ) -> Result<u64, Error> {
        let mut move_file = self.0.clone();
        logwise::perfwarn_begin!("async_file uses blocking on this platform");
        unblock(move || {
            let pos = move_file.seek(pos);
            match pos {
                Ok(pos) => Ok(pos),
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(|e| e.into())
    }

    pub async fn metadata(&self, _priority: Priority) -> Result<Metadata, Error> {
        let move_file = self.0.clone();
        logwise::perfwarn_begin!("async_file uses blocking on this platform");

        unblock(move || {
            let metadata = move_file.metadata();
            metadata.map(Metadata)
        })
        .await
        .map_err(|e| e.into())
    }
}

//boilerplate impls

impl PartialEq for Data {
    /// Compares two `Data` instances for equality.
    ///
    /// Two `Data` instances are equal if their underlying byte slices contain
    /// the same bytes in the same order.
    ///
    /// # Example
    ///
    /// ```
    /// # use async_file::Priority;
    /// # async fn example() -> Result<(), async_file::Error> {
    /// use async_file::File;
    ///
    /// let file1 = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let data1 = file1.read(10, Priority::unit_test()).await?;
    ///
    /// let file2 = File::open("/dev/zero", Priority::unit_test()).await?;
    /// let data2 = file2.read(10, Priority::unit_test()).await?;
    ///
    /// // Both reads from /dev/zero return all zeros
    /// assert_eq!(data1, data2);
    /// # Ok(())
    /// # }
    /// # test_executors::spin_on(example()).unwrap();
    /// ```
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

pub async fn exists(path: impl AsRef<Path>, _priority: Priority) -> bool {
    let path = path.as_ref().to_owned();
    logwise::perfwarn_begin!("async_file uses blocking on this platform");
    unblock(move || path.exists()).await
}

/// Sets the default origin for file operations (no-op in std implementation).
///
/// This function exists for API compatibility with the WASM implementation,
/// where it sets the origin URL for fetching files. In the standard library
/// implementation, this is a no-op since files are accessed directly from
/// the file system.
///
/// # Arguments
///
/// * `_path` - Path parameter (unused in std implementation)
///
/// # Returns
///
/// Always returns `Ok(())` since there's nothing to configure.
///
/// # Example
///
/// ```
/// # use async_file::Priority;
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # struct DummyError;
/// # impl std::fmt::Display for DummyError {
/// #     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) }
/// # }
/// # impl std::fmt::Debug for DummyError {
/// #     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) }
/// # }
/// # impl std::error::Error for DummyError {}
/// # fn set_default_origin(_: impl AsRef<std::path::Path>) -> Result<(), DummyError> { Ok(()) }
/// // This is a no-op on non-WASM platforms
/// set_default_origin("/some/path")?;
///
/// // File operations work the same regardless
/// # Ok(())
/// # }
/// # example().unwrap();
/// ```
///
/// # Platform Differences
///
/// - **Standard (this implementation)**: No-op, always succeeds
/// - **WASM**: Sets the base URL for fetching remote files
///
/// This allows code to be written that works on both platforms without
/// conditional compilation.
pub fn set_default_origin(_path: impl AsRef<Path>) {
    //nothing to do here, as std impl does not use origins
}
