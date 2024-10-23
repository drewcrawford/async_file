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
}

#[derive(Debug)]
pub struct Error(sys::Error);



#[cfg(test)] mod tests {
    use crate::File;

    #[test]
    fn test_open_file() {
        test_executors::spin_on(async {
            let file = File::open("/dev/zero").await.unwrap();
        });
    }
}