use std::{collections::HashSet, hash::Hash, num::NonZeroU32, rc::Rc};

use crate::{parser::DiceRollSource, Result, Rollable};

/// A kind of dice which can be rolled.
pub trait DiceKind: Copy {
    type Roll: Roll;
    fn roll(&self, rng: &mut impl DiceRollSource) -> Self::Roll;
    fn max(&self) -> Self::Roll;
    fn min(&self) -> Self::Roll;
}

/// Allow using a NonZeroU32 as a fair dice from 1 to self inclusive.
impl DiceKind for NonZeroU32 {
    type Roll = u32;

    fn roll(&self, rng: &mut impl DiceRollSource) -> Self::Roll {
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

pub trait Roll: Ord + Into<i64> + Copy + Hash {}

/// A [Fudge_dice](https://en.wikipedia.org/wiki/Fudge_%28role-playing_game_system%29#Fudge_dice).
#[derive(Debug, Ord, Eq, Copy, PartialEq, Clone, PartialOrd)]
struct Fudge;

#[derive(Debug, Ord, Eq, Copy, PartialEq, Clone, PartialOrd, Hash)]
struct FudgeRoll {
    // Always -1, 0 or 1
    value: i8,
}

impl FudgeRoll {
    pub fn new(rng: &mut impl DiceRollSource) -> Self {
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

impl Roll for u32 {}

impl DiceKind for Fudge {
    type Roll = FudgeRoll;

    fn roll(&self, rng: &mut impl DiceRollSource) -> Self::Roll {
        FudgeRoll::new(rng)
    }
    fn max(&self) -> Self::Roll {
        FudgeRoll { value: 1 }
    }
    fn min(&self) -> Self::Roll {
        FudgeRoll { value: -1 }
    }
}

/// A batch of rolls of the same kind of dice.
#[derive(Debug, Clone)]
pub(crate) struct RollBatch<Dice: DiceKind + Clone> {
    pub dice: Dice,
    pub rolls: Vec<Dice::Roll>,
}

/// A batch of rolls of the same kind of dice.
#[derive(Debug, Clone)]
pub(crate) struct ModifiedRollBatch<TRoll> {
    pub rolls: Vec<ModifiedRoll<TRoll>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ModifiedRoll<TRoll> {
    before: TRoll,
    modifier: RollModifier<TRoll>,
}

impl<TRoll: Copy> ModifiedRoll<TRoll> {
    pub fn after(&self) -> Vec<TRoll> {
        match &self.modifier {
            RollModifier::None => vec![self.before],
            RollModifier::Drop => vec![],
            RollModifier::Reroll(r) => vec![r.last().unwrap_or(&self.before).clone()],
            RollModifier::Explode(r) => {
                let mut v = vec![self.before];
                v.extend(r);
                v
            }
        }
    }
}

impl<TRoll: Roll> ModifiedRollBatch<TRoll> {
    pub fn new<Dice: DiceKind<Roll = TRoll>>(
        batch: &RollBatch<Dice>,
        modifier: RollBatchModifier<TRoll>,
        rng: &mut impl DiceRollSource,
    ) -> Result<Self> {
        let rolls = match modifier {
            RollBatchModifier::KeepOrDrop(op) => batch.keep_or_drop(op)?,
            RollBatchModifier::PerRollModifier(op) => {
                let mut modified = vec![];
                for before in &batch.rolls {
                    modified.push(op.apply(batch.dice, *before, rng)?)
                }
                ModifiedRollBatch { rolls: modified }
            }
        };

        Ok(rolls)
    }
    pub fn after(&self) -> Vec<TRoll> {
        self.rolls.iter().flat_map(|x| x.after()).collect()
    }
}

/// See ModifiedRoll.after for how to apply this to a roll.
#[derive(Debug, Clone)]
enum RollModifier<Roll> {
    // keep original
    None,
    /// Dice dropped, and should not be counted.
    Drop,
    /// Original was rerolled (each reroll in the vec), and should be replaced with last entry in this Vec (keep original if empty)
    Reroll(Vec<Roll>),
    /// Original was exploded and should have every item in the vec added as another dice.
    Explode(Vec<Roll>),
}

/// A modifier that can be applied to a RollBatch
#[derive(Debug, Clone, Copy)]
enum RollBatchModifier<Roll> {
    KeepOrDrop(KeepOrDrop),
    PerRollModifier(PerRollModifier<Roll>),
}

/// A modifier that can be applied to a RollBatch
#[derive(Debug, Clone, Copy)]
enum PerRollModifier<Roll> {
    /// Reroll dice equal or lower than this value once
    RerollOnce(Roll),
    /// Reroll dice equal or lower than this value iteratively
    RerollUnlimited(Roll),
    /// Explode dice equal or greater than this value once
    ExplodeOnce(Roll),
    /// Explode dice equal or greater than this value iteratively
    ExplodeUnlimited(Roll),
}

impl<TRoll: Roll> PerRollModifier<TRoll> {
    pub fn apply<Dice: DiceKind<Roll = TRoll>>(
        &self,
        dice: Dice,
        roll: TRoll,
        rng: &mut impl DiceRollSource,
    ) -> Result<ModifiedRoll<TRoll>> {
        let modifier = match self {
            PerRollModifier::RerollOnce(n) => {
                if roll <= *n {
                    RollModifier::Reroll(vec![dice.roll(rng)])
                } else {
                    RollModifier::None
                }
            }
            PerRollModifier::RerollUnlimited(n) => {
                if *n >= dice.max() {
                    return Err("Infinite rerolls".into());
                }
                let new_rolls = roll_until(dice, roll, |next| next > *n, rng);
                if new_rolls.len() > 0 {
                    RollModifier::Reroll(new_rolls)
                } else {
                    RollModifier::None
                }
            }
            PerRollModifier::ExplodeOnce(n) => {
                if roll >= *n {
                    RollModifier::Explode(vec![dice.roll(rng)])
                } else {
                    RollModifier::None
                }
            }
            PerRollModifier::ExplodeUnlimited(n) => {
                if *n <= dice.min() {
                    return Err("Infinite explodes".into());
                }
                let new_rolls = roll_until(dice, roll, |next| next < *n, rng);
                if new_rolls.len() > 0 {
                    RollModifier::Explode(new_rolls)
                } else {
                    RollModifier::None
                }
            }
        };

        Ok(ModifiedRoll {
            modifier,
            before: roll,
        })
    }
}

/// Rolls until end_condition is true for a roll value.
/// Returns all new rolls.
/// May return empty if condition was true for provided roll.
fn roll_until<Dice: DiceKind>(
    dice: Dice,
    mut roll: Dice::Roll,
    end_condition: impl Fn(Dice::Roll) -> bool,
    rng: &mut impl DiceRollSource,
) -> Vec<Dice::Roll> {
    let mut new_rolls = vec![];
    loop {
        if end_condition(roll) {
            return new_rolls;
        }
        roll = dice.roll(rng);
        new_rolls.push(roll);
    }
}

impl<Dice: DiceKind + Clone> RollBatch<Dice> {
    pub fn keep_or_drop(&self, op: KeepOrDrop) -> Result<ModifiedRollBatch<Dice::Roll>> {
        let rolls = op.apply(&self.rolls, |d| *d)?;

        Ok(ModifiedRollBatch {
            rolls: rolls
                .iter()
                .map(|(keep, value)| ModifiedRoll {
                    before: *value,
                    modifier: match keep {
                        true => RollModifier::None,
                        false => RollModifier::Drop,
                    },
                })
                .collect(),
        })
    }
}

/// Specification for a single batch of dice to roll and process.
pub struct RollSpec<Dice: DiceKind> {
    dice: Dice,
    number_of_dice: usize,
    modifiers: Vec<RollBatchModifier<Dice::Roll>>,
    aggregator: Aggregator<Dice::Roll>,
}

impl<Dice: DiceKind> Rollable for RollSpec<Dice> {
    type Roll = Result<EvaluatedRollSpec<Dice>>;

    fn roll_with_source(&self, rng: &mut impl DiceRollSource) -> Result<EvaluatedRollSpec<Dice>> {
        let mut rolls = RollBatch {
            rolls: (0..self.number_of_dice)
                .map(|_| self.dice.roll(rng))
                .collect(),
            dice: self.dice,
        };

        let mut history: Vec<(RollBatchModifier<Dice::Roll>, ModifiedRollBatch<Dice::Roll>)> =
            vec![];

        for modifier in &self.modifiers {
            let next = ModifiedRollBatch::new(&rolls, *modifier, rng)?;
            rolls.rolls = next.after();
            history.push((modifier.clone(), next));
        }

        Ok(EvaluatedRollSpec {
            total: self.aggregator.total(&rolls.rolls),
            history,
            final_rolls: rolls,
        })
    }
}

pub struct EvaluatedRollSpec<Dice: DiceKind> {
    total: i64,
    /// All modifications applied to the batch of rolls. Empty of none.
    history: Vec<(RollBatchModifier<Dice::Roll>, ModifiedRollBatch<Dice::Roll>)>,
    /// The final dice, after apply all modifications.
    ///
    /// Same as `.after()` for last entry in history (when history is not empty).
    final_rolls: RollBatch<Dice>,
}

// number represent nb dice to keep/drop
#[derive(Clone)]
pub(crate) enum Aggregator<TRoll> {
    /// These values are in order:
    /// (target (threshold for success),
    /// failure (threshold for negative success),
    /// target doubled (threshold for two successes per dice))
    TargetFailureDouble(TRoll, TRoll, TRoll),
    // List of specific values which count as success
    TargetEnum(HashSet<TRoll>),
    Sum,
}

impl<TRoll: Roll> Aggregator<TRoll> {
    pub fn total(&self, rolls: &[TRoll]) -> i64 {
        rolls.iter().fold(0, |sum, r| sum + self.apply_single(*r))
    }

    pub fn apply_single(&self, roll: TRoll) -> i64 {
        match self {
            Aggregator::TargetFailureDouble(t, f, d) => {
                if roll >= *d {
                    2
                } else if roll >= *t {
                    1
                } else if roll <= *f {
                    -1
                } else {
                    0
                }
            }
            Aggregator::TargetEnum(items) => {
                if items.contains(&roll) {
                    1
                } else {
                    0
                }
            }
            Aggregator::Sum => Into::<i64>::into(roll),
        }
    }
}

/// Number of dice to keep or drop.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) enum KeepOrDrop {
    KeepHi(usize),
    KeepLo(usize),
    DropHi(usize),
    DropLo(usize),
}

impl KeepOrDrop {
    pub fn apply<T: Clone, Key: Ord + Copy>(
        &self,
        v: &[T],
        get_number: impl Fn(&T) -> Key,
    ) -> Result<Vec<(bool, T)>> {
        let res = match self {
            KeepOrDrop::KeepHi(n) => {
                keep_low(&v, *n, |result| std::cmp::Reverse(get_number(&result)))?
            }
            KeepOrDrop::KeepLo(n) => keep_low(&v, *n, |result| get_number(&result))?,
            KeepOrDrop::DropHi(n) => keep_low(&v, v.len() - n, |result| get_number(&result))?,
            KeepOrDrop::DropLo(n) => keep_low(&v, v.len() - n, |result| {
                std::cmp::Reverse(get_number(&result))
            })?,
        };
        Ok(res)
    }
}

/// Copy `v`, but with the top (as defined by `f`) `to_drop` entries flagged with false and the rest with true.
fn keep_low<T: Clone, Key: Ord + Copy>(
    v: &[T],
    to_keep: usize,
    f: impl Fn(&T) -> Key,
) -> Result<Vec<(bool, T)>> {
    if to_keep > v.len() {
        return Err("Not enough dice to keep or drop".into());
    }

    // [(sort_value, original_index)]
    let mut keys: Vec<(Key, usize)> = v.iter().enumerate().map(|(i, t)| (f(t), i)).collect();
    keys.sort_by_key(|(sort_value, _original_index)| *sort_value);
    let mut flagged_indexes: Vec<(bool, usize)> = keys
        .iter()
        .enumerate()
        .map(|(i, (_, index))| (i < to_keep, *index))
        .collect();
    flagged_indexes.sort_by_key(|(_keep, key)| *key);

    Ok(flagged_indexes
        .iter()
        .map(|(flag, index)| (*flag, v[*index].clone()))
        .collect())
}

#[cfg(test)]
mod tests {
    use crate::tests::IteratorDiceRollSource;

    use super::*;

    #[test]
    fn keep_low_test() {
        assert_eq!(
            keep_low(&[1, 3, 2], 2, |x| *x).unwrap(),
            vec![(true, 1), (false, 3), (true, 2)]
        );
        assert_eq!(
            keep_low(&[1, 3, 2], 2, |x| -*x).unwrap(),
            vec![(false, 1), (true, 3), (true, 2)]
        );
        assert_eq!(
            keep_low(&[4, 1, 3, 2], 2, |x| *x).unwrap(),
            vec![(false, 4), (true, 1), (false, 3), (true, 2)]
        );
        assert_eq!(
            keep_low(&[4, 1, 3, 2], 1, |x| *x).unwrap(),
            vec![(false, 4), (true, 1), (false, 3), (false, 2)]
        );
        assert_eq!(keep_low(&[4], 1, |x| *x).unwrap(), vec![(true, 4)]);
    }

    const D20: NonZeroU32 = NonZeroU32::new(20).unwrap();

    #[test]
    fn smoke() {
        let spec = RollSpec {
            dice: D20,
            number_of_dice: 2,
            modifiers: vec![],
            aggregator: Aggregator::Sum,
        };
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        assert_eq!(result.total, 3);
    }

    #[test]
    fn keep() {
        let spec = RollSpec {
            dice: D20,
            number_of_dice: 4,
            modifiers: vec![RollBatchModifier::KeepOrDrop(KeepOrDrop::KeepHi(2))],
            aggregator: Aggregator::Sum,
        };
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        assert_eq!(result.total, 7);
    }
}
