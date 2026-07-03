use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum StorageError {
    #[error("item not found")]
    NotFound,

    #[error("item already exists")]
    AlreadyExists,
}
