//! Kinds of dice
//!
//! Implementations of [DiceKind] and its output trait [Roll].

use std::{
    error::Error,
    fmt::{self, Display},
    hash::Hash,
    num::{IntErrorKind, ParseFloatError, ParseIntError},
    str::FromStr,
};

use crate::{DiceRollSource, RollError};

/// A kind of dice which can be rolled.
pub(crate) trait DiceKind:
    Copy + FromStr<Err: fmt::Debug> + 'static + fmt::Debug + Display
{
    type Roll: Roll;
    fn roll(&self, rng: &mut dyn DiceRollSource) -> Self::Roll;
    fn max(&self) -> Self::Roll;
    fn min(&self) -> Self::Roll;
}

pub(crate) trait Roll:
    Ord + Into<i64> + Copy + Hash + Display + FromStr<Err: fmt::Debug> + fmt::Debug
{
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParseDiceError {
    kind: IntErrorKind,
}
/// based on ParseIntError: https://doc.rust-lang.org/src/core/num/error.rs.html#123
impl Display for ParseDiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            IntErrorKind::Empty => "cannot parse integer from empty string",
            IntErrorKind::InvalidDigit => "invalid digit found in string",
            IntErrorKind::PosOverflow => "number too large to fit in target type",
            IntErrorKind::NegOverflow => "number too small to fit in target type",
            IntErrorKind::Zero => "number would be zero for non-zero type",
            _ => "unknown error",
        }
        .fmt(f)
    }
}
impl Error for ParseDiceError {}

impl From<ParseIntError> for ParseDiceError {
    fn from(value: ParseIntError) -> Self {
        ParseDiceError {
            kind: *value.kind(),
        }
    }
}

impl From<ParseDiceError> for RollError {
    fn from(value: ParseDiceError) -> Self {
        RollError::parse_error(Box::new(value))
    }
}

impl From<ParseIntError> for RollError {
    fn from(e: ParseIntError) -> Self {
        RollError::parse_error(e)
    }
}

impl From<ParseFloatError> for RollError {
    fn from(e: ParseFloatError) -> Self {
        RollError::parse_error(e)
    }
}

// Implementations of DiceKind
pub(crate) mod basic;
pub(crate) mod fudge;
