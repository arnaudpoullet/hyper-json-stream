use std::str::FromStr;

use crate::JsonStreamError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContentEncoding {
    None,
    Gzip,
}

impl FromStr for ContentEncoding {
    type Err = JsonStreamError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gzip" => Ok(ContentEncoding::Gzip),
            _ => Ok(ContentEncoding::None),
        }
    }
}
