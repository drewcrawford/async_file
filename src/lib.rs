/*!
afile is a simple file I/O library for Rust.
*/

mod std_impl;

use std::path::Path;
use std_impl as sys;

pub struct File(sys::File);

impl File {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self,Error> {
        sys::File::open(path).await.map(File).map_err(Error)
    }
    pub async fn read(&mut self, buf_size: usize) -> Result<(usize, Box<[u8]>), Error> {
        self.0.read(buf_size).await.map_err(Error)
    }

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