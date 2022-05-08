use std::{path::Path, fs::{File, Metadata}, io::Write};
use async_tar::{Archive, Builder};
use futures::{Stream, AsyncWrite};
use std::io::Read;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CacheErrors<'a> {
    #[error("Directory of file at {path:?} does not exist")]
    FileLocationDoesNotExist {
        path: &'a Path
    },
    #[error("Directory of file at {path:?} does not have a parent directory")]
    DirectoryWithoutParent {
        path: &'a Path,
    },
    #[error("Directory of file at {path:?} is read-only")]
    FileLocationIsReadOnly {
        path: &'a Path,
        metadata: Metadata,
    }
}

pub enum CacheType {
    GithubActions,
    File(&'static Path),
    Directory(&'static Path)
}


pub struct Cache  {
    cache_type: CacheType,
    archive: Option<Builder<async_std::fs::File>>
}

impl Cache {
    pub async fn new<'a>(cache_type: CacheType) -> Result<Self, CacheErrors<'a>> {
        let mut value = Self {
            cache_type,
            archive: None,
        };

        if let Err(error) = value.validate() {
            return Err(error);
        }

        if let CacheType::File(file_path) = value.cache_type {
            value.archive = Some(Builder::new(async_std::fs::File::open(file_path).await.unwrap()));
        }

        Ok(value)
    }
}

impl<'a> Cache {
    fn validate(&self) -> Result<(), CacheErrors<'a>> {
        match self.cache_type {
            CacheType::GithubActions => Ok(()),
            CacheType::File(file) => {
                if let Some(parent) = file.parent() {
                    if !parent.exists() {
                        return Err(CacheErrors::FileLocationDoesNotExist {
                            path: parent,
                        });
                    } else {
                        if let Ok(metadata) = parent.metadata() {
                            if metadata.permissions().readonly() {
                                return Err(CacheErrors::FileLocationIsReadOnly {
                                    path: parent,
                                    metadata
                                })
                            }
                        }
                    }
                } else {
                    return Err(CacheErrors::DirectoryWithoutParent {
                        path: file,
                    });
                }
                return Ok(());
            },
            CacheType::Directory(path) => {
                if let Some(parent) = path.parent() {
                    if !parent.exists() {
                        return Err(CacheErrors::FileLocationDoesNotExist {
                            path: parent,
                        });
                    } else {
                        if let Ok(metadata) = parent.metadata() {
                            if metadata.permissions().readonly() {
                                return Err(CacheErrors::FileLocationIsReadOnly {
                                    path: parent,
                                    metadata
                                })
                            }
                        }
                    }
                } else {
                    return Err(CacheErrors::DirectoryWithoutParent {
                        path: path,
                    });
                }
                return Ok(());
            }
        }
    }

    pub fn cache_file(&self, file_name: String, input: impl Stream<Item = std::io::Result<bytes::Bytes>> + Unpin + tokio::io::AsyncRead) {
        match self.cache_type {
            CacheType::Directory(_) => {
                unimplemented!();
            },
            CacheType::File(_) => {
                self.archive.unwrap().append_file(file_name,  tokio_util::io::ReaderStream::new(input));
            },
            CacheType::GithubActions => {
                unimplemented!();
            }
        }
        /* Archive::new(
            tokio_util::io::ReaderStream::new(
                async_compression::tokio::bufread::GzipDecoder::new(StreamReader::new(
                    input
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
                )),
            )
            .into_async_read(),
        ); */
    }

}