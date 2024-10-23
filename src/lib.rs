/*!
afile is a simple file I/O library for Rust.
*/

mod std_impl;

use std::hash::Hash;
use std::path::Path;
use std_impl as sys;

#[derive(Debug)]
pub struct File(sys::File);
pub type Priority = priority::Priority;

/**
An opaque buffer type.

# Design

Imagine the OS implements reasonable syscalls like `read` by passing you a pointer into
kernel memory.  You can read the memory, but you can't write it, but you do need to free it.

You can copy it into a vec or something and free it straightaway, but the copy may be undesireable.

For reading we can import into Rust as a slice.  But for freeing we need a custom Drop implementation,
and we can't Drop a reference; that has no effect.

So here's an opaque type that can be bridged to the slice but may have a custom Drop implementation.
To bridge to slice, use `as_ref` or `deref`.  To bridge to a `Box<[u8]>`, use `into_boxed_slice`.


*/
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
    /**
    Converts into a boxed slice.

    # Performance

    On platforms where it is sensible, this is a zero-cost operation.
    On platforms where it isn't, this may require a copy.

    Consider the pros and cons of converting to a boxed slice vs keeping the Data type around.

    */
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
    pub async fn open(path: impl AsRef<Path>, priority: Priority) -> Result<Self,Error> {
        sys::File::open(path, priority).await.map(File).map_err(Error)
    }
    /**

    Reads up to `buf_size` bytes from the file.  Compare with `std::fs::File::read`.

    Only one operation may be in-flight at a time.

    Some differences from `fs` are:
       * Memory is allocated by the OS instead of by you.

         Imagine that you start a read operation.  After starting it, you cancel the operation (drop a future).
         Maybe we do some cancel logic, but also our OS is still writing into the buffer for awhile.  The best
         way to handle this is for the OS to also control the allocation.

       * The buffer is returned as an opaque type, [Data].
    */
    pub async fn read(&self, buf_size: usize, priority: Priority) -> Result<Data, Error> {
        self.0.read(buf_size,priority).await.map(Data).map_err(Error)
    }

    /**
    Seeks to a position in the file.  Compare with `std::fs::File::seek`.

    Only one operation may be in-flight at a time.
    */
    pub async fn seek(&mut self, pos: std::io::SeekFrom, priority: Priority) -> Result<u64, Error> {
        self.0.seek(pos,priority).await.map_err(Error)
    }

    /**
    Returns metadata about the file.  Compare with `std::fs::File::metadata`.
*/
    pub async fn metadata(&self, priority: Priority) -> Result<Metadata, Error> {
        self.0.metadata(priority).await.map(Metadata).map_err(Error)
    }
}

#[derive(Debug)]
#[derive(thiserror::Error)]
#[error("afile error {0}")]
pub struct Error(sys::Error);

#[derive(Debug)]
pub struct Metadata(sys::Metadata);
impl Metadata {
    /**
    Returns the length of the file in bytes.
    */
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



#[cfg(test)] mod tests {
    use crate::{Data, File, Metadata, Priority};

    #[test]
    fn test_open_file() {
        logwise::context::Context::reset("test_open_file");
        test_executors::spin_on(async {
            let _file = File::open("/dev/zero", Priority::unit_test()).await.unwrap();
        });
    }
    #[test]
    fn test_read_file() {
        logwise::context::Context::reset("test_read_file");
        test_executors::spin_on(async {
            let mut file = File::open("/dev/zero", Priority::unit_test()).await.unwrap();
            let buf = file.read(1024, Priority::unit_test()).await.unwrap();
            assert_eq!(buf.len(), 1024);
            assert_eq!(buf.iter().all(|&x| x == 0), true);
        });
    }

    #[test]
    fn test_seek_file() {
        logwise::context::Context::reset("test_seek_file");
        test_executors::spin_on(async {
            let mut file = File::open("/dev/zero", Priority::unit_test()).await.unwrap();
            let pos = file.seek(std::io::SeekFrom::Start(1024), Priority::unit_test()).await.unwrap();
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

    #[test] fn test_unpin() {
        fn _assert_unpin<T: Unpin>() {}
        _assert_unpin::<Data>();
        _assert_unpin::<File>();
        _assert_unpin::<Metadata>();
    }

    #[test] fn test_length() {
        logwise::context::Context::reset("test_length");
        test_executors::spin_on(async {
            let mut file = File::open("/dev/zero", Priority::unit_test()).await.unwrap();
            let metadata = file.metadata(Priority::unit_test()).await.unwrap();
            assert_eq!(metadata.len(), 0);
        });
    }
}