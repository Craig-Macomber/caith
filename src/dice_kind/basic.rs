use crate::DiceRollSource;

use crate::dice_kind::{DiceKind, Roll};

use std::num::NonZeroU32;

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
