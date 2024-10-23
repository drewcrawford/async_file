use std::io::Read;
use std::path::Path;
use blocking::unblock;
use std::io::Seek;
use std::ops::Deref;
use crate::Priority;

/**
stdlib-based implementation*/

pub struct File(Option<std::fs::File>);

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error)
}

pub struct Data(Box<[u8]>);

impl AsRef<[u8]> for Data {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Deref for Data {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl Data {
    pub fn into_boxed_slice(self) -> Box<[u8]> {
        self.0
    }
}

impl File {
    fn new(file: std::fs::File) -> Self {
        File(Some(file))
    }
    pub async fn open(path: impl AsRef<Path>, _priority: Priority) -> Result<Self, Error> {
        logwise::perfwarn_begin!("afile uses blocking on this platform");
        let path = path.as_ref().to_owned();
        unblock(|| std::fs::File::open(path)).await.map(File::new).map_err(|e| e.into())
    }

    pub async fn read(&mut self, buf_size: usize, _priority: Priority) -> Result<(usize, Data), Error> {
        let mut move_file = self.0.take().expect("File operation in-flight already");
        logwise::perfwarn_begin!("afile uses blocking on this platform");
        unblock(move || {
            let mut buf = vec![0; buf_size];
            let read = move_file.read(&mut buf);
            match read {
                Ok(read) => {
                    Ok((move_file, read, buf.into_boxed_slice()))
                }
                Err(e) => {
                    Err(e)
                }
            }
        }).await.map(|(file, read, buf)| {
            self.0 = Some(file);
            let data = Data(buf);

            (read, data)
        }).map_err(|e| e.into())
    }

    pub async fn seek(&mut self, pos: std::io::SeekFrom, _priority: Priority) -> Result<u64, Error> {
        let mut move_file = self.0.take().expect("File operation in-flight already");
        logwise::perfwarn_begin!("afile uses blocking on this platform");
        unblock(move || {
            let pos = move_file.seek(pos);
            match pos {
                Ok(pos) => {
                    Ok((move_file, pos))
                }
                Err(e) => {
                    Err(e)
                }
            }
        }).await.map(|(file, pos)| {
            self.0 = Some(file);
            pos
        }).map_err(|e| e.into())
    }
}