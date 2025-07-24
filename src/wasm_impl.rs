//SPDX-License-Identifier: MIT OR Apache-2.0
use crate::Priority;
use std::ops::Deref;
use std::path::Path;

/**
WASM-based implementation*/

#[derive(Debug)]
pub struct File;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("WASM I/O error: {0}")]
    Wasm(String),
}

#[derive(Debug)]
pub struct Data(Box<[u8]>);

#[derive(Debug, Clone)]
pub struct Metadata {
    len: u64,
}

impl Metadata {
    pub fn len(&self) -> u64 {
        self.len
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
    pub async fn open(_path: impl AsRef<Path>, _priority: Priority) -> Result<Self, Error> {
        todo!()
    }

    pub async fn read(&self, _buf_size: usize, _priority: Priority) -> Result<Data, Error> {
        todo!()
    }

    pub async fn seek(
        &mut self,
        _pos: std::io::SeekFrom,
        _priority: Priority,
    ) -> Result<u64, Error> {
        todo!()
    }

    pub async fn metadata(&self, _priority: Priority) -> Result<Metadata, Error> {
        todo!()
    }
}

//boilerplate impls

impl PartialEq for Data {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::hash::Hash for Data {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

pub async fn exists(_path: impl AsRef<Path>, _priority: Priority) -> bool {
    todo!()
}