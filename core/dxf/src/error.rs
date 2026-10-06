// SPDX-License-Identifier: MIT
use std::fmt;

/// A DXF file that cannot be read or a drawing that cannot be written.
#[derive(Debug)]
pub struct Error {
    /// 1-based line of the file where reading failed, if known.
    pub line: Option<usize>,
    pub message: String,
}

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }

    pub(crate) fn at(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "DXF line {line}: {}", self.message),
            None => write!(f, "DXF: {}", self.message),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}
