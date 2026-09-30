use std::{
    error::Error as StdError,
    fmt::{self, Display},
    result,
};

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    // boxed as it is much larger than every other variant
    Serenity(Box<serenity::Error>),
    Rusqlite(rusqlite::Error),
    Url(url::ParseError),
    Reqwest(reqwest::Error),
    Join(tokio::task::JoinError),
    BotMessage,
    ConstStr(&'static str),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Serenity(inner) => Display::fmt(inner, f),
            Error::Rusqlite(inner) => Display::fmt(inner, f),
            Error::Url(inner) => Display::fmt(inner, f),
            Error::Reqwest(inner) => Display::fmt(inner, f),
            Error::Join(inner) => Display::fmt(inner, f),
            Error::ConstStr(inner) => f.write_str(inner),
            Error::BotMessage => f.write_str("Message is from a bot"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Serenity(inner) => Some(inner.as_ref()),
            Error::Rusqlite(inner) => Some(inner),
            Error::Url(inner) => Some(inner),
            Error::Reqwest(inner) => Some(inner),
            Error::Join(inner) => Some(inner),
            Error::BotMessage | Error::ConstStr(_) => None,
        }
    }
}

impl From<serenity::Error> for Error {
    fn from(e: serenity::Error) -> Error {
        Error::Serenity(Box::new(e))
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

impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Error {
        Error::Join(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display() {
        assert_eq!(Error::BotMessage.to_string(), "Message is from a bot");
        assert_eq!(Error::ConstStr("some error").to_string(), "some error");
        let url_err = Error::from(url::ParseError::EmptyHost);
        assert_eq!(url_err.to_string(), url::ParseError::EmptyHost.to_string());
    }

    #[test]
    fn test_source() {
        assert!(Error::BotMessage.source().is_none());
        assert!(
            Error::from(rusqlite::Error::QueryReturnedNoRows)
                .source()
                .is_some()
        );
    }

    #[test]
    fn test_error_is_small() {
        // keep Result<T> cheap to move around, see clippy::result_large_err
        assert!(size_of::<Error>() <= 64, "{}", size_of::<Error>());
    }
}
