//! Kinds of dice
//!
//! Implementations of [DiceKind] and its output trait [Roll].

use std::{
    error::Error,
    fmt::{self, Display},
    num::{IntErrorKind, NonZeroU32, ParseFloatError, ParseIntError},
    str::FromStr,
};

use crate::{
    dice_kind::{DiceKind, Roll},
    parser::DiceRollSource,
    RollError,
};

impl Roll for u32 {}

/// This is using an intentionally limited
pub type BasicDice = NonZeroU32;

/// Allow using a BasicDice as a fair dice from 1 to self inclusive.
impl DiceKind for BasicDice {
    type Roll = u32;

    fn roll(&self, rng: &mut dyn DiceRollSource) -> Self::Roll {
        let value = rng.roll_single_die(self.get().into());
        <u64 as TryInto<u32>>::try_into(value).unwrap()
    }
    fn max(&self) -> Self::Roll {
        (*self).into()
    }
    fn min(&self) -> Self::Roll {
        1
    }
}

/// A [Fudge_dice](https://en.wikipedia.org/wiki/Fudge_%28role-playing_game_system%29#Fudge_dice).
#[derive(Debug, Ord, Eq, Copy, PartialEq, Clone, PartialOrd)]
pub(crate) struct Fudge;

#[derive(Debug, Ord, Eq, Copy, PartialEq, Clone, PartialOrd, Hash)]
pub(crate) struct FudgeRoll {
    // Always -1, 0 or 1
    value: i8,
}

impl Display for FudgeRoll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            1 => write!(f, "(+)"),
            0 => write!(f, "( )"),
            -1 => write!(f, "(-)"),
            _ => unreachable!(),
        }
    }
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
        RollError::ParseError(Box::new(value))
    }
}

impl From<ParseIntError> for RollError {
    fn from(e: ParseIntError) -> Self {
        RollError::ParseError(Box::new(e))
    }
}

impl From<ParseFloatError> for RollError {
    fn from(e: ParseFloatError) -> Self {
        RollError::ParseError(Box::new(e))
    }
}

impl FudgeRoll {
    pub fn new(rng: &mut dyn DiceRollSource) -> Self {
        let value = rng.roll_single_die(3);
        FudgeRoll {
            value: <u64 as TryInto<i8>>::try_into(value).unwrap() - 2,
        }
    }
}

impl Into<i64> for FudgeRoll {
    fn into(self) -> i64 {
        self.value.into()
    }
}
impl Roll for FudgeRoll {}

impl FromStr for FudgeRoll {
    type Err = ParseDiceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "(+)" {
            return Ok(FudgeRoll { value: 1 });
        }
        if s == "( )" {
            return Ok(FudgeRoll { value: 0 });
        }
        if s == "(-)" {
            return Ok(FudgeRoll { value: -1 });
        }

        let value = s.parse::<i8>()?;
        if value > 1 {
            Err(ParseDiceError {
                kind: IntErrorKind::PosOverflow,
            })
        } else if value < -1 {
            Err(ParseDiceError {
                kind: IntErrorKind::NegOverflow,
            })
        } else {
            Ok(FudgeRoll { value })
        }
    }
}

impl FromStr for Fudge {
    type Err = ParseDiceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "F" || s == "f" {
            Ok(Fudge)
        } else {
            Err(ParseDiceError {
                kind: IntErrorKind::InvalidDigit,
            })
        }
    }
}

impl Display for Fudge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("F")
    }
}

impl DiceKind for Fudge {
    type Roll = FudgeRoll;

    fn roll(&self, rng: &mut dyn DiceRollSource) -> Self::Roll {
        FudgeRoll::new(rng)
    }
    fn max(&self) -> Self::Roll {
        FudgeRoll { value: 1 }
    }
    fn min(&self) -> Self::Roll {
        FudgeRoll { value: -1 }
    }
}
