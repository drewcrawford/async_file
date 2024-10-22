use std::path::Path;

/**
stdlib-based implementation*/

pub struct File {

}

#[derive(Debug)]
pub enum Error {

}

impl File {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self,Error> {
        todo!()
    }
}