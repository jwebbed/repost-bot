use rusqlite;
use serenity;
use url;

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
    Url(url::ParseError),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Serenity(inner) => fmt::Display::fmt(&inner, f),
            Error::Rusqlite(inner) => fmt::Display::fmt(&inner, f),
            Error::Url(inner) => fmt::Display::fmt(&inner, f),
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

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Error {
        Error::Url(e)
    }
}
