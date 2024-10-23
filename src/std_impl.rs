use std::future::Future;
use std::io::Read;
use std::os::unix::raw::mode_t;
use std::path::Path;
use std::sync::Arc;
use blocking::unblock;

/**
stdlib-based implementation*/

pub struct File(Option<std::fs::File>);

#[derive(Debug,thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error)
}

impl File {
    fn new(file: std::fs::File) -> Self {
        File(Some(file))
    }
    pub async fn open(path: impl AsRef<Path>) -> Result<Self,Error> {
        logwise::perfwarn_begin!("afile uses blocking on this platform");
        let path = path.as_ref().to_owned();
        unblock(|| std::fs::File::open(path)).await.map(File::new).map_err(|e| e.into())
    }

    fn read(&mut self,buf_size: usize) -> impl Future<Output=Result<(usize,Box<[u8]>),Error>> + Send + use<'_> {
        let mut move_file = self.0.take().expect("File operation in-flight already");
        async move {
            logwise::perfwarn_begin!("afile uses blocking on this platform");

            unblock(move || {
                let mut buf = vec![0; buf_size];
                let read = move_file.read(&mut buf);
                match read {
                    Ok(read) => {
                        Ok((move_file,read, buf.into_boxed_slice()))
                    }
                    Err(e) => {
                        Err(e)
                    }
                }
            }).await.map(|(file, read,buf)| {
                self.0 = Some(file);
                (read,buf)
            }).map_err(|e| e.into())
        }
    }

}