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
    Internal(String),
    BotMessage,
    ConstStr(&'static str),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Serenity(inner) => fmt::Display::fmt(&inner, f),
            Error::Rusqlite(inner) => fmt::Display::fmt(&inner, f),
            Error::Url(inner) => fmt::Display::fmt(&inner, f),
            Error::Internal(inner) => f.write_str(inner),
            Error::ConstStr(inner) => f.write_str(inner),
            Error::BotMessage => f.write_str("Message is from a bot"),
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
