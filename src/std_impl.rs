use std::io::Read;
use std::path::Path;
use blocking::unblock;
use std::io::Seek;
use std::ops::Deref;
use std::sync::Arc;
use crate::Priority;

/**
stdlib-based implementation*/

#[derive(Debug)]
pub struct File(Arc<std::fs::File>);

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error)
}

#[derive(Debug)]
pub struct Data(Box<[u8]>);

#[derive(Debug)]
pub struct Metadata(std::fs::Metadata);

impl Metadata {
    pub fn len(&self) -> u64 {
        self.0.len()
    }
}


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
        File(Arc::new(file))
    }
    pub async fn open(path: impl AsRef<Path>, _priority: Priority) -> Result<Self, Error> {
        logwise::perfwarn_begin!("afile uses blocking on this platform");
        let path = path.as_ref().to_owned();
        unblock(|| std::fs::File::open(path)).await.map(File::new).map_err(|e| e.into())
    }

    pub async fn read(&self, buf_size: usize, _priority: Priority) -> Result<Data, Error> {
        let mut move_file = self.0.clone();
        logwise::perfwarn_begin!("afile uses blocking on this platform");
        unblock(move || {
            let mut buf = vec![0; buf_size];
            let read = move_file.read(&mut buf);
            match read {
                Ok(read) => {
                    buf.truncate(read);
                    Ok(buf.into_boxed_slice())
                }
                Err(e) => {
                    Err(e)
                }
            }
        }).await.map(Data).map_err(|e| e.into())
    }

    pub async fn seek(&mut self, pos: std::io::SeekFrom, _priority: Priority) -> Result<u64, Error> {
        let mut move_file = self.0.clone();
        logwise::perfwarn_begin!("afile uses blocking on this platform");
        unblock(move || {
            let pos = move_file.seek(pos);
            match pos {
                Ok(pos) => {
                    Ok(pos)
                }
                Err(e) => {
                    Err(e)
                }
            }
        }).await.map_err(|e| e.into())
    }

    pub async fn metadata(&self, _priority: Priority) -> Result<Metadata, Error> {
        let move_file = self.0.clone();
        logwise::perfwarn_begin!("afile uses blocking on this platform");

        unblock(move || {
            let metadata = move_file.metadata();
            metadata.map(|m| Metadata(m))
        }).await.map_err(|e| e.into())
    }
}

//boilerplate impls

impl PartialEq for Data {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

pub async fn exists(path: impl AsRef<Path>, _priority: Priority) -> bool {
    let path = path.as_ref().to_owned();
    logwise::perfwarn_begin!("afile uses blocking on this platform");
    unblock(move || path.exists()).await
}


