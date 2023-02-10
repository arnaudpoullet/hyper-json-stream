use hyper::StatusCode;
use std::fmt;
use std::string::FromUtf8Error;

/// Parse the content length header.
pub fn get_content_length(parts: &http::response::Parts) -> usize {
    parts
        .headers
        .get(http::header::CONTENT_LENGTH)
        .and_then(|size_str| size_str.to_str().ok())
        .and_then(|size_str| size_str.parse().ok())
        .unwrap_or(0)
}

#[derive(Debug)]
#[non_exhaustive]
pub enum JsonStreamError {
    HyperError(hyper::Error),
    HttpError(http::Error),
    IOError(std::io::Error),
    JsonError(serde_json::Error),
    ApiError(StatusCode, String),
    /// This type is only returned if the format of the json downloaded is wrong.
    MalformedJson(String),
}

/// Load errors
impl JsonStreamError {
    pub(crate) fn json(s: String) -> JsonStreamError {
        JsonStreamError::MalformedJson(s)
    }
}

impl From<serde_json::Error> for JsonStreamError {
    fn from(err: serde_json::Error) -> JsonStreamError {
        JsonStreamError::JsonError(err)
    }
}
impl From<hyper::Error> for JsonStreamError {
    fn from(err: hyper::Error) -> JsonStreamError {
        JsonStreamError::HyperError(err)
    }
}
impl From<http::Error> for JsonStreamError {
    fn from(err: http::Error) -> JsonStreamError {
        JsonStreamError::HttpError(err)
    }
}
impl From<http::header::InvalidHeaderName> for JsonStreamError {
    fn from(err: http::header::InvalidHeaderName) -> JsonStreamError {
        JsonStreamError::HttpError(http::Error::from(err))
    }
}
impl From<http::header::InvalidHeaderValue> for JsonStreamError {
    fn from(err: http::header::InvalidHeaderValue) -> JsonStreamError {
        JsonStreamError::HttpError(http::Error::from(err))
    }
}
impl From<http::uri::InvalidUri> for JsonStreamError {
    fn from(err: http::uri::InvalidUri) -> JsonStreamError {
        JsonStreamError::HttpError(http::Error::from(err))
    }
}
impl From<FromUtf8Error> for JsonStreamError {
    fn from(value: FromUtf8Error) -> Self {
        JsonStreamError::json(value.to_string())
    }
}
impl From<std::io::Error> for JsonStreamError {
    fn from(err: std::io::Error) -> JsonStreamError {
        JsonStreamError::IOError(err)
    }
}
impl fmt::Display for JsonStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonStreamError::HyperError(err) => err.fmt(f),
            JsonStreamError::HttpError(err) => err.fmt(f),
            JsonStreamError::IOError(err) => err.fmt(f),
            JsonStreamError::JsonError(err) => err.fmt(f),
            JsonStreamError::ApiError(status, err) => {
                write!(f, "{} : {}", status, err)
            }
            JsonStreamError::MalformedJson(ref msg) => msg.fmt(f),
        }
    }
}
impl std::error::Error for JsonStreamError {
    fn cause(&self) -> Option<&dyn std::error::Error> {
        match self {
            JsonStreamError::HyperError(err) => Some(err),
            JsonStreamError::HttpError(err) => Some(err),
            JsonStreamError::IOError(err) => Some(err),
            JsonStreamError::JsonError(err) => Some(err),
            JsonStreamError::ApiError(_, _) => None,
            JsonStreamError::MalformedJson(_) => None,
        }
    }
}
