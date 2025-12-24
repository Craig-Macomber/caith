//! Implementation of [Expression] for the `dice` rule in the grammar.

use std::{
    collections::HashSet,
    fmt::{Debug, Display},
    str::FromStr,
};

use pest::iterators::{Pair, Pairs};

use crate::{
    DiceRollSource, Result, RollError, Rollable,
    dice_kind::{DiceKind, Roll, basic::BasicDice, fudge::Fudge},
    expression::{
        EvaluatedExpression, Expression, ExpressionResult, ExpressionRollable, Verbosity,
    },
    keep_or_drop::KeepOrDrop,
    parser::Rule,
};

/// A batch of rolls of the same kind of dice.
#[derive(Debug, Clone)]
struct RollBatch<Dice: DiceKind + Clone> {
    dice: Dice,
    rolls: Vec<Dice::Roll>,
}

/// A batch of rolls of the same kind of dice.
#[derive(Debug, Clone)]
struct ModifiedRollBatch<TRoll> {
    rolls: Vec<ModifiedRoll<TRoll>>,
}

#[derive(Debug, Clone)]
struct ModifiedRoll<TRoll> {
    before: TRoll,
    modifier: RollModifier<TRoll>,
}

impl<TRoll: Roll> ModifiedRoll<TRoll> {
    fn format(&self, markdown: bool) -> String {
        if markdown {
            match &self.modifier {
                RollModifier::None => format!("{}", self.before),
                RollModifier::Drop => format!("~~*{}*~~", self.before),
                RollModifier::Reroll(items) => {
                    format!(
                        "{}{}",
                        format_join(
                            self.chain(items.clone()).map(|x| format!("~~*{}*~~🡲", x)),
                            ""
                        ),
                        items.last().unwrap()
                    )
                }
                RollModifier::Explode(items) => {
                    format!(
                        "{}{}",
                        format_join(
                            self.chain(items.clone())
                                // For reasons unknown, some markdown parsers need a space after the ** here to parse correctly for fudge dice which format with a space in them.
                                // To fix this without undesired visual impact include a zero width space to fix it.
                                // This zero width space has to be escaped instead of included directly for it to have an effect.
                                .map(|x| format!("**{}**&#x200B;🡵", x)),
                            ""
                        ),
                        items.last().unwrap()
                    )
                }
            }
        } else {
            match &self.modifier {
                RollModifier::None => format!("{}", self.before),
                RollModifier::Drop => format!("Drop({})", self.before),
                RollModifier::Reroll(items) => {
                    format!(
                        "{}{}",
                        format_join(
                            self.chain(items.clone()).map(|x| format!("{}🡲Reroll🡲", x)),
                            ""
                        ),
                        items.last().unwrap()
                    )
                }
                RollModifier::Explode(items) => {
                    format!(
                        "{}{}",
                        format_join(
                            self.chain(items.clone())
                                .map(|x| format!("{}(Exploded)🡵", x)),
                            ""
                        ),
                        items.last().unwrap()
                    )
                }
            }
        }
    }

    /// Iterate over before then all but the last item in items
    fn chain(&self, items: Vec<TRoll>) -> impl Iterator<Item = TRoll> {
        let len = items.len();
        Some(self.before).into_iter().chain(items).take(len)
    }
}

impl<TRoll: Copy> ModifiedRoll<TRoll> {
    fn after(&self) -> Vec<TRoll> {
        match &self.modifier {
            RollModifier::None => vec![self.before],
            RollModifier::Drop => vec![],
            RollModifier::Reroll(r) => vec![*r.last().unwrap_or(&self.before)],
            RollModifier::Explode(r) => {
                let mut v = vec![self.before];
                v.extend(r);
                v
            }
        }
    }
}

impl<TRoll: Roll> ModifiedRollBatch<TRoll> {
    fn new<Dice: DiceKind<Roll = TRoll>>(
        batch: &RollBatch<Dice>,
        modifier: RollBatchModifier<TRoll>,
        rng: &mut dyn DiceRollSource,
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
    fn after(&self) -> Vec<TRoll> {
        self.rolls.iter().flat_map(|x| x.after()).collect()
    }
}

/// See [ModifiedRoll::after] for how to apply this to a roll.
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

impl<TRoll: Roll> Display for RollBatchModifier<TRoll> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RollBatchModifier::KeepOrDrop(keep_or_drop) => std::fmt::Display::fmt(&keep_or_drop, f),
            RollBatchModifier::PerRollModifier(per_roll_modifier) => {
                Display::fmt(&per_roll_modifier, f)
            }
        }
    }
}

impl Display for KeepOrDrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeepOrDrop::KeepHi(u) => write!(f, "K{u}"),
            KeepOrDrop::KeepLo(u) => write!(f, "k{u}"),
            KeepOrDrop::DropHi(u) => write!(f, "D{u}"),
            KeepOrDrop::DropLo(u) => write!(f, "d{u}"),
        }
    }
}

impl<TRoll: Roll> Display for PerRollModifier<TRoll> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PerRollModifier::RerollOnce(r) => write!(f, "r{r}"),
            PerRollModifier::RerollUnlimited(r) => write!(f, "ir{r}"),
            PerRollModifier::ExplodeOnce(r) => write!(f, "e{r}"),
            PerRollModifier::ExplodeUnlimited(r) => write!(f, "!{r}"),
        }
    }
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
    fn apply<Dice: DiceKind<Roll = TRoll>>(
        &self,
        dice: Dice,
        roll: TRoll,
        rng: &mut dyn DiceRollSource,
    ) -> Result<ModifiedRoll<TRoll>> {
        let max = dice.max();
        let min = dice.min();
        let modifier = match self {
            PerRollModifier::RerollOnce(n) => {
                if roll <= *n {
                    RollModifier::Reroll(vec![dice.roll(rng)])
                } else {
                    RollModifier::None
                }
            }
            PerRollModifier::RerollUnlimited(n) => {
                if *n >= max {
                    // TODO: catch this during parse
                    return Err(RollError::ParamError(format!(
                        "Cannot infinitely reroll dice of {n} or lower since the maximum roll is {max}: this would go on forever"
                    )));
                }
                let new_rolls = roll_until(dice, roll, |next| next > *n, rng)?;
                if !new_rolls.is_empty() {
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
                    // TODO: catch this during parse
                    return Err(RollError::ParamError(format!(
                        "Cannot infinitely explode dice of {n} or higher since the minimum roll is {min}: this would go on forever"
                    )));
                }
                let new_rolls = roll_until(dice, roll, |next| next < *n, rng)?;
                if !new_rolls.is_empty() {
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
    rng: &mut dyn DiceRollSource,
) -> Result<Vec<Dice::Roll>> {
    let mut new_rolls = vec![];
    loop {
        if end_condition(roll) {
            return Ok(new_rolls);
        }
        limit_dice(new_rolls.len(), "rerolls")?;
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
#[derive(Debug)]
struct RollSpec<Dice: DiceKind> {
    dice: Dice,
    number_of_dice: usize,
    modifiers: Vec<RollBatchModifier<Dice::Roll>>,
    aggregator: Aggregator<Dice::Roll>,
}

impl<Dice: DiceKind> Display for RollSpec<Dice> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let modifiers = self
            .modifiers
            .iter()
            .map(|m| format!(" {m}"))
            .collect::<Vec<_>>()
            .join("");
        let aggregator = match &self.aggregator {
            Aggregator::TargetFailureDouble(t, f, tt) => format!(
                "{}{}{}",
                match t {
                    Some(n) => format!(" t{n}"),
                    None => "".to_string(),
                },
                match f {
                    Some(n) => format!(" f{n}"),
                    None => "".to_string(),
                },
                match tt {
                    Some(n) => format!(" tt{n}"),
                    None => "".to_string(),
                }
            ),
            Aggregator::TargetEnum(hash_set) => {
                let mut items = hash_set.iter().collect::<Vec<_>>();
                items.sort();
                format!(
                    " t[{}]",
                    items
                        .iter()
                        .map(|n| format!("{n}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Aggregator::Sum => "".to_string(),
        };
        write!(
            f,
            "{}d{}{modifiers}{aggregator}",
            self.number_of_dice, self.dice,
        )
    }
}

impl<Dice: DiceKind> ExpressionRollable for RollSpec<Dice> {
    fn expression_roll(&self, rng: &mut dyn DiceRollSource) -> ExpressionResult {
        let x = self.dyn_roll(rng)?;
        let boxed: Box<dyn EvaluatedExpression> = Box::new(x);
        Ok(boxed)
    }
}

// Arbitrary limits to avoid OOM and hangs
const MAX_NUMBER_OF_DICE: usize = 5_000;

pub(crate) fn limit_dice(number_of_dice: usize, during: &str) -> Result<()> {
    if number_of_dice > MAX_NUMBER_OF_DICE {
        Err(format!(
            "Exceed maximum allowed number of dice ({MAX_NUMBER_OF_DICE}) during {during}.",
        )
        .into())
    } else {
        Ok(())
    }
}

impl<Dice: DiceKind> RollSpec<Dice> {
    fn dyn_roll(&self, rng: &mut dyn DiceRollSource) -> Result<EvaluatedRollSpec<Dice>> {
        let mut rolls = RollBatch {
            rolls: (0..self.number_of_dice)
                .map(|_| self.dice.roll(rng))
                .collect(),
            dice: self.dice,
        };

        let mut history: History<Dice::Roll> = vec![];

        for modifier in &self.modifiers {
            let next = ModifiedRollBatch::new(&rolls, *modifier, rng)?;
            rolls.rolls = next.after();
            limit_dice(rolls.rolls.len(), "batch aggregation")?;
            history.push((*modifier, next));
        }

        Ok(EvaluatedRollSpec {
            total: self.aggregator.total(&rolls.rolls),
            history,
            final_rolls: rolls,
        })
    }
}

impl<Dice: DiceKind> Rollable for RollSpec<Dice> {
    type Roll = Result<EvaluatedRollSpec<Dice>>;

    fn roll_with_source(&self, rng: &mut dyn DiceRollSource) -> Result<EvaluatedRollSpec<Dice>> {
        self.dyn_roll(rng)
    }
}

type History<Roll> = Vec<(RollBatchModifier<Roll>, ModifiedRollBatch<Roll>)>;

#[derive(Debug)]
struct EvaluatedRollSpec<Dice: DiceKind> {
    total: i64,
    /// All modifications applied to the batch of rolls. Empty of none.
    history: History<Dice::Roll>,
    /// The final dice, after apply all modifications.
    ///
    /// Same as `.after()` for last entry in history (when history is not empty).
    final_rolls: RollBatch<Dice>,
}

impl<Dice: DiceKind> EvaluatedExpression for EvaluatedRollSpec<Dice> {
    fn total(&self) -> f64 {
        self.total as f64
    }

    fn format_history(&self, markdown: bool, verbose: Verbosity) -> String {
        if let Some(first) = self.history.first() {
            if matches!(verbose, Verbosity::Short) {
                let original = first.1.rolls.iter().map(|m| m.before);
                format!(
                    "{} 🡲 {}",
                    format_rolls(original, markdown),
                    format_rolls(self.final_rolls.rolls.iter(), markdown)
                )
            } else {
                let mut stages = vec![];
                for s in &self.history {
                    let rolls =
                        format_rolls(s.1.rolls.iter().map(|m| m.format(markdown)), markdown);
                    let stage = format!("{}{}", rolls, s.0);
                    stages.push(stage);
                }

                if matches!(verbose, Verbosity::Verbose) {
                    stages.push(format_rolls(self.final_rolls.rolls.iter(), markdown));
                }

                stages.join(" 🡲 ")
            }
        } else {
            format_rolls(self.final_rolls.rolls.iter(), markdown)
        }
    }
}

fn format_rolls<I: Iterator>(rolls: I, markdown: bool) -> String
where
    I::Item: Display,
{
    if markdown {
        format!("\\[{}\\]", format_join(rolls, ", "))
    } else {
        format!("[{}]", format_join(rolls, ", "))
    }
}

fn format_join<I: Iterator>(rolls: I, sep: &str) -> String
where
    I::Item: Display,
{
    rolls
        .map(|r| format!("{}", r))
        .collect::<Vec<_>>()
        .join(sep)
}

// number represent nb dice to keep/drop
#[derive(Clone, Debug)]
enum Aggregator<TRoll> {
    /// These values are in order:
    /// (target (threshold for success),
    /// failure (threshold for negative success),
    /// target doubled (threshold for two successes per dice))
    TargetFailureDouble(Option<TRoll>, Option<TRoll>, Option<TRoll>),
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
                if let Some(d) = *d
                    && roll >= d
                {
                    return 2;
                }
                if let Some(t) = *t
                    && roll >= t
                {
                    return 1;
                }
                if let Some(f) = *f
                    && roll <= f
                {
                    return -1;
                }
                0
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

pub(crate) fn parse_dice<Dice: DiceKind>(mut dice: Pairs<Rule>) -> Result<Expression> {
    let number_of_dice = dice.next().unwrap();
    let number_of_dice = match number_of_dice.as_rule() {
        Rule::number_of_dice => {
            dice.next(); // skip `d` token
            number_of_dice.as_str().parse::<usize>()?
        }
        Rule::roll => 1, // no number before `d`, assume 1 dice
        _ => unreachable!("{:?}", number_of_dice),
    };

    limit_dice(number_of_dice, "parse")?;

    let pair = dice.next().unwrap();
    match pair.as_rule() {
        Rule::number => {
            parse_dice_inner::<BasicDice>(pair.as_str().parse::<BasicDice>()?, number_of_dice, dice)
        }
        Rule::fudge => parse_dice_inner::<Fudge>(Fudge, number_of_dice, dice),
        _ => unreachable!("{:?}", pair),
    }
}

fn extract_option_value<T: FromStr<Err: Debug>>(option: Pair<Rule>) -> Result<Option<T>>
where
    RollError: From<T::Err>,
{
    let mut inner = option.into_inner();
    let next = inner.next();
    let x = match next {
        Some(p) => Some(p.as_str().parse::<T>()?),
        None => None,
    };
    Ok(x)
}

pub(crate) fn parse_dice_inner<Dice: DiceKind>(
    dice_parsed: Dice,
    number_of_dice: usize,
    mut dice: Pairs<Rule>,
) -> Result<Expression>
where
    RollError: From<<Dice::Roll as FromStr>::Err>,
{
    let sides = dice_parsed.max();

    let mut modifiers: Vec<RollBatchModifier<Dice::Roll>> = vec![];

    let mut aggregator: Aggregator<Dice::Roll> = Aggregator::Sum;
    let mut next_option = dice.next();

    while next_option.is_some() {
        let option = next_option.unwrap();

        match &option.as_rule() {
            Rule::explode => {
                let value = extract_option_value(option)?.unwrap_or(sides);
                modifiers.push(RollBatchModifier::PerRollModifier(
                    PerRollModifier::ExplodeOnce(value),
                ));
            }
            Rule::i_explode => {
                let value = extract_option_value(option)?.unwrap_or(sides);
                modifiers.push(RollBatchModifier::PerRollModifier(
                    PerRollModifier::ExplodeUnlimited(value),
                ));
            }
            Rule::reroll => {
                let value = extract_option_value(option)?.unwrap();
                modifiers.push(RollBatchModifier::PerRollModifier(
                    PerRollModifier::RerollOnce(value),
                ));
            }
            Rule::i_reroll => {
                let value = extract_option_value(option)?.unwrap();
                modifiers.push(RollBatchModifier::PerRollModifier(
                    PerRollModifier::RerollUnlimited(value),
                ));
            }
            Rule::keep_hi => {
                let value = extract_option_value::<usize>(option)?.unwrap();
                modifiers.push(RollBatchModifier::KeepOrDrop(KeepOrDrop::KeepHi(value)));
            }
            Rule::keep_lo => {
                let value = extract_option_value::<usize>(option)?.unwrap();
                modifiers.push(RollBatchModifier::KeepOrDrop(KeepOrDrop::KeepLo(value)));
            }
            Rule::drop_hi => {
                let value = extract_option_value::<usize>(option)?.unwrap();
                modifiers.push(RollBatchModifier::KeepOrDrop(KeepOrDrop::DropHi(value)));
            }
            Rule::drop_lo => {
                let value = extract_option_value::<usize>(option)?.unwrap();
                modifiers.push(RollBatchModifier::KeepOrDrop(KeepOrDrop::DropLo(value)));
            }
            Rule::target => {
                let value_or_enum = option.into_inner().next().unwrap();
                match value_or_enum.as_rule() {
                    Rule::number | Rule::fudge_value => {
                        let value = value_or_enum.as_str().parse::<Dice::Roll>()?;
                        let (double_target, fail) = match aggregator {
                            Aggregator::TargetFailureDouble(None, f, tt) => (tt, f),
                            Aggregator::Sum => (None, None),
                            _ => Err("Invalid targets 1")?,
                        };
                        aggregator =
                            Aggregator::TargetFailureDouble(Some(value), fail, double_target)
                    }

                    Rule::target_enum => {
                        let numbers_list = value_or_enum.into_inner();
                        let numbers_list: Vec<Dice::Roll> = numbers_list
                            .map(|p| p.as_str().parse::<Dice::Roll>())
                            .collect::<std::result::Result<Vec<Dice::Roll>, <Dice::Roll as FromStr>::Err>>()?;
                        aggregator =
                            Aggregator::TargetEnum(HashSet::from_iter(numbers_list.into_iter()))
                    }
                    _ => unreachable!(),
                };
            }
            Rule::double_target => {
                let value = extract_option_value(option)?.unwrap();
                let (target, fail) = match aggregator {
                    Aggregator::TargetFailureDouble(t, f, None) => (t, f),
                    Aggregator::Sum => (None, None),
                    _ => Err("Invalid targets 2")?,
                };
                aggregator = Aggregator::TargetFailureDouble(target, fail, Some(value))
            }
            Rule::failure => {
                let value = extract_option_value(option)?.unwrap();
                let (target, double_target) = match aggregator {
                    Aggregator::TargetFailureDouble(t, None, d) => (t, d),
                    Aggregator::Sum => (None, None),
                    _ => Err("Invalid targets 3")?,
                };
                aggregator = Aggregator::TargetFailureDouble(target, Some(value), double_target)
            }
            _ => unreachable!("{:#?}", option),
        }

        next_option = dice.next();
    }

    Ok(Expression::new(RollSpec {
        dice: dice_parsed,
        number_of_dice,
        modifiers,
        aggregator,
    }))
}

#[cfg(test)]
mod tests {
    use crate::tests::IteratorDiceRollSource;

    use super::*;

    const D20: BasicDice = BasicDice::new(20).unwrap();

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

    #[test]
    fn format() {
        let spec = RollSpec {
            dice: D20,
            number_of_dice: 4,
            modifiers: vec![
                RollBatchModifier::KeepOrDrop(KeepOrDrop::KeepHi(2)),
                RollBatchModifier::PerRollModifier(PerRollModifier::ExplodeOnce(1)),
                RollBatchModifier::KeepOrDrop(KeepOrDrop::DropLo(1)),
            ],
            aggregator: Aggregator::Sum,
        };
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        assert_eq!(
            result.format_history(false, Verbosity::Short),
            "[1, 2, 3, 4] 🡲 [5, 4, 6]"
        );
        assert_eq!(
            result.format_history(true, Verbosity::Short),
            "\\[1, 2, 3, 4\\] 🡲 \\[5, 4, 6\\]"
        );
        assert_eq!(
            result.format_history(false, Verbosity::Medium),
            "[Drop(1), Drop(2), 3, 4]K2 🡲 [3(Exploded)🡵5, 4(Exploded)🡵6]e1 🡲 [Drop(3), 5, 4, 6]d1"
        );
        assert_eq!(
            result.format_history(true, Verbosity::Medium),
            "\\[~~*1*~~, ~~*2*~~, 3, 4\\]K2 🡲 \\[**3**&#x200B;🡵5, **4**&#x200B;🡵6\\]e1 🡲 \\[~~*3*~~, 5, 4, 6\\]d1"
        );

        assert_eq!(
            result.format_history(false, Verbosity::Verbose),
            "[Drop(1), Drop(2), 3, 4]K2 🡲 [3(Exploded)🡵5, 4(Exploded)🡵6]e1 🡲 [Drop(3), 5, 4, 6]d1 🡲 [5, 4, 6]"
        );
        assert_eq!(
            result.format_history(true, Verbosity::Verbose),
            "\\[~~*1*~~, ~~*2*~~, 3, 4\\]K2 🡲 \\[**3**&#x200B;🡵5, **4**&#x200B;🡵6\\]e1 🡲 \\[~~*3*~~, 5, 4, 6\\]d1 🡲 \\[5, 4, 6\\]"
        );
    }
}
