use std::{
    fmt::{self, Display},
    num::IntErrorKind,
    str::FromStr,
};

use crate::{
    DiceRollSource,
    dice_kind::ParseDiceError,
    dice_kind::{DiceKind, Roll},
};

/// A [Fudge_dice](https://en.wikipedia.org/wiki/Fudge_%28role-playing_game_system%29#Fudge_dice).
#[derive(Debug, Ord, Eq, Copy, PartialEq, Clone, PartialOrd)]
pub(crate) struct Fudge;

#[derive(Debug, Ord, Eq, Copy, PartialEq, Clone, PartialOrd, Hash)]
pub(crate) struct FudgeRoll {
    // Always -1, 0 or 1
    pub(crate) value: i8,
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

impl FudgeRoll {
    pub fn new(rng: &mut dyn DiceRollSource) -> Self {
        let value = rng.roll_single_die(3);
        FudgeRoll {
            value: <u64 as TryInto<i8>>::try_into(value).unwrap() - 2,
        }
    }
}

impl From<FudgeRoll> for i64 {
    fn from(val: FudgeRoll) -> Self {
        val.value.into()
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
