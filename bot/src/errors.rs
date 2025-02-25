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
    Reqwest(reqwest::Error),
    Image(image::ImageError),
    Io(std::io::Error),
    BotMessage,
    ConstStr(&'static str),
    BoxedErr(Box<Error>),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Serenity(inner) => fmt::Display::fmt(&inner, f),
            Error::Rusqlite(inner) => fmt::Display::fmt(&inner, f),
            Error::Url(inner) => fmt::Display::fmt(&inner, f),
            Error::Reqwest(inner) => fmt::Display::fmt(&inner, f),
            Error::Image(inner) => fmt::Display::fmt(&inner, f),
            Error::Io(inner) => fmt::Display::fmt(&inner, f),
            Error::BoxedErr(inner) => fmt::Display::fmt(&inner, f),
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

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Error {
        Error::Reqwest(e)
    }
}

impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Error {
        Error::Image(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<&Error> for Error {
    fn from(e: &Error) -> Error {
        Error::BoxedErr(Box::new(e.into()))
    }
}
