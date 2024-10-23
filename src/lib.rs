/*!
afile is a simple file I/O library for Rust.
*/

mod std_impl;

use std::path::Path;
use std_impl as sys;

pub struct File(sys::File);

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
    pub async fn open(path: impl AsRef<Path>) -> Result<Self,Error> {
        sys::File::open(path).await.map(File).map_err(Error)
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
    pub async fn read(&mut self, buf_size: usize) -> Result<(usize, Data), Error> {
        self.0.read(buf_size).await.map(|(read, data)| (read, Data(data))).map_err(Error)
    }

    /**
    Seeks to a position in the file.  Compare with `std::fs::File::seek`.

    Only one operation may be in-flight at a time.
    */
    pub async fn seek(&mut self, pos: std::io::SeekFrom) -> Result<u64, Error> {
        self.0.seek(pos).await.map_err(Error)
    }
}

#[derive(Debug)]
#[derive(thiserror::Error)]
#[error("afile error {0}")]
pub struct Error(sys::Error);



#[cfg(test)] mod tests {
    use crate::File;

    #[test]
    fn test_open_file() {
        logwise::context::Context::reset("test_open_file");
        test_executors::spin_on(async {
            let _file = File::open("/dev/zero").await.unwrap();
        });
    }
    #[test]
    fn test_read_file() {
        logwise::context::Context::reset("test_read_file");
        test_executors::spin_on(async {
            let mut file = File::open("/dev/zero").await.unwrap();
            let (read, buf) = file.read(1024).await.unwrap();
            assert_eq!(read, 1024);
            assert_eq!(buf.len(), 1024);
            assert_eq!(buf.iter().all(|&x| x == 0), true);
        });
    }

    #[test]
    fn test_seek_file() {
        test_executors::spin_on(async {
            let mut file = File::open("/dev/zero").await.unwrap();
            let pos = file.seek(std::io::SeekFrom::Start(1024)).await.unwrap();
            assert_eq!(pos, 1024);
        });
    }
}