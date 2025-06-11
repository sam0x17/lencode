use super::prelude::*;
use ::serde::{de, ser};
use core::fmt::Display;

impl ser::Error for Error {
    fn custom<T: core::fmt::Display>(msg: T) -> Self {
        Error::Serde(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Serde(msg.to_string())
    }
}
