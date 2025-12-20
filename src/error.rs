use std::fmt::Debug;
use std::rc::Rc;
use std::{error::Error, fmt::Display};

use crate::parser::*;

/// Result type used across the library
pub type Result<T> = std::result::Result<T, RollError>;

/// The error reported
/// Comparison is by pointer if boxing a parse error.
#[derive(Debug, Clone)]
pub enum RollError {
    /// Error while parsing the expression, emitted by `pest`
    ParseError(Rc<dyn Error>),
    /// Any other error while walking the AST, the String contains an explanation of what happened
    ParamError(String),
}

impl PartialEq for RollError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ParseError(l0), Self::ParseError(r0)) => Rc::ptr_eq(l0, r0),
            (Self::ParamError(l0), Self::ParamError(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl RollError {
    pub(crate) fn parse_error(e: impl Error + 'static) -> RollError {
        RollError::ParseError(Rc::new(e))
    }
}

impl Display for RollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RollError::ParseError(e) => write!(f, "{}", e),
            RollError::ParamError(e) => write!(f, "{}", e),
        }
    }
}

impl Error for RollError {}

impl From<pest::error::Error<Rule>> for RollError {
    fn from(e: pest::error::Error<Rule>) -> Self {
        RollError::parse_error(e)
    }
}

impl From<&str> for RollError {
    fn from(e: &str) -> Self {
        RollError::ParamError(e.to_string())
    }
}

impl From<String> for RollError {
    fn from(e: String) -> Self {
        Self::ParamError(e)
    }
}
