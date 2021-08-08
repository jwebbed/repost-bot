use rusqlite;
use serenity;

use std::{
    error::Error as StdError,
    fmt::{self, Display},
    result,
};

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Serenity(serenity::Error),
    Rusqlite(rusqlite::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Serenity(inner) => fmt::Display::fmt(&inner, f),
            Error::Rusqlite(inner) => fmt::Display::fmt(&inner, f),
        }
    }
}

impl StdError for Error {}

impl From<serenity::Error> for Error {
    fn from(e: serenity::Error) -> Error {
        Error::Serenity(e)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Error {
        Error::Rusqlite(e)
    }
}
