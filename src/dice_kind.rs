use std::{collections::HashSet, fmt::Display, hash::Hash, num::NonZeroU32};

use pest::{
    iterators::{Pair, Pairs},
    Parser,
};

use crate::{
    parser::{get_climber, keep_low, DiceRollSource, RollParser, Rule},
    Result, Rollable,
};

/// A kind of dice which can be rolled.
pub(crate) trait DiceKind: Copy {
    type Roll: Roll;
    fn roll(&self, rng: &mut dyn DiceRollSource) -> Self::Roll;
    fn max(&self) -> Self::Roll;
    fn min(&self) -> Self::Roll;
}

pub(crate) trait Roll: Ord + Into<i64> + Copy + Hash + Display {}

/// A batch of rolls of the same kind of dice.
#[derive(Debug, Clone)]
struct RollBatch<Dice: DiceKind + Clone> {
    pub dice: Dice,
    pub rolls: Vec<Dice::Roll>,
}

/// A batch of rolls of the same kind of dice.
#[derive(Debug, Clone)]
struct ModifiedRollBatch<TRoll> {
    pub rolls: Vec<ModifiedRoll<TRoll>>,
}

#[derive(Debug, Clone)]
struct ModifiedRoll<TRoll> {
    before: TRoll,
    modifier: RollModifier<TRoll>,
}

impl<TRoll: Roll> ModifiedRoll<TRoll> {
    pub fn format(&self, markdown: bool) -> String {
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
                        format_join(self.chain(items.clone()).map(|x| format!("**{}**🡵", x)), ""),
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
        Some(self.before)
            .into_iter()
            .chain(items.into_iter())
            .take(len)
    }
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
    pub fn after(&self) -> Vec<TRoll> {
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
            RollBatchModifier::KeepOrDrop(keep_or_drop) => keep_or_drop.fmt(f),
            RollBatchModifier::PerRollModifier(per_roll_modifier) => per_roll_modifier.fmt(f),
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
    pub fn apply<Dice: DiceKind<Roll = TRoll>>(
        &self,
        dice: Dice,
        roll: TRoll,
        rng: &mut dyn DiceRollSource,
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
    rng: &mut dyn DiceRollSource,
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
struct RollSpec<Dice: DiceKind> {
    dice: Dice,
    number_of_dice: usize,
    modifiers: Vec<RollBatchModifier<Dice::Roll>>,
    aggregator: Aggregator<Dice::Roll>,
}

/// A parsed dice expression.
pub struct Expression(Box<dyn ExpressionRollable>);

pub type ExpressionResult = Result<Box<dyn EvaluatedExpression>>;

trait ExpressionRollable {
    /// Evaluate and roll the dice with provided dice roll source
    fn expression_roll(&self, rng: &mut dyn DiceRollSource) -> ExpressionResult;
}

impl Rollable for Expression {
    type Roll = ExpressionResult;

    fn roll_with_source(&self, rng: &mut dyn DiceRollSource) -> Self::Roll {
        let inner: &dyn ExpressionRollable = &*self.0;
        ExpressionRollable::expression_roll(inner, rng)
    }
}

impl Expression {
    /// Parse as string into an [Expression].
    pub fn parse(expression: &str) -> Result<Expression> {
        parse_single_command(expression)
    }

    fn new<T: ExpressionRollable + 'static>(expression: T) -> Expression {
        Expression(Box::new(expression))
    }
}

impl<Dice: DiceKind + 'static> ExpressionRollable for RollSpec<Dice> {
    fn expression_roll(&self, rng: &mut dyn DiceRollSource) -> ExpressionResult {
        let x = self.dyn_roll(rng)?;
        let boxed: Box<dyn EvaluatedExpression> = Box::new(x);
        Ok(boxed)
    }
}

#[derive(Clone, Copy)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl BinaryOp {
    fn apply(&self, left: f64, right: f64) -> f64 {
        match self {
            BinaryOp::Add => left + right,
            BinaryOp::Sub => left - right,
            BinaryOp::Mul => left * right,
            BinaryOp::Div => left / right,
        }
    }

    fn format<T: Display>(&self, left: T, right: T) -> String {
        format!(
            "{left} {} {right}",
            match self {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
            }
        )
    }
}

struct BinaryExpression<T> {
    left: T,
    op: BinaryOp,
    right: T,
}

impl ExpressionRollable for BinaryExpression<Expression> {
    fn expression_roll(&self, rng: &mut dyn DiceRollSource) -> ExpressionResult {
        let left = self.left.roll_with_source(rng)?;
        let right = self.right.roll_with_source(rng)?;
        Ok(Box::new(BinaryExpression {
            left,
            op: self.op,
            right,
        }))
    }
}

impl EvaluatedExpression for BinaryExpression<Box<dyn EvaluatedExpression>> {
    fn total(&self) -> f64 {
        self.op.apply(self.left.total(), self.right.total())
    }

    fn format_history(&self, markdown: bool, verbose: Verbosity) -> String {
        self.op.format(
            self.left.format_history(markdown, verbose),
            self.right.format_history(markdown, verbose),
        )
    }
}

impl ExpressionRollable for f64 {
    fn expression_roll(&self, _rng: &mut dyn DiceRollSource) -> ExpressionResult {
        Ok(Box::new(*self))
    }
}

impl EvaluatedExpression for f64 {
    fn total(&self) -> f64 {
        *self
    }

    fn format_history(&self, _markdown: bool, _verbose: Verbosity) -> String {
        format!("{self}")
    }
}

impl ExpressionRollable for i64 {
    fn expression_roll(&self, _rng: &mut dyn DiceRollSource) -> ExpressionResult {
        Ok(Box::new(*self))
    }
}

impl EvaluatedExpression for i64 {
    fn total(&self) -> f64 {
        *self as f64
    }

    fn format_history(&self, _markdown: bool, _verbose: Verbosity) -> String {
        format!("{self}")
    }
}

struct BlockExpression<T> {
    inner: T,
}

impl ExpressionRollable for BlockExpression<Expression> {
    fn expression_roll(&self, rng: &mut dyn DiceRollSource) -> ExpressionResult {
        Ok(Box::new(BlockExpression {
            inner: self.inner.roll_with_source(rng)?,
        }))
    }
}

impl EvaluatedExpression for BlockExpression<Box<dyn EvaluatedExpression>> {
    fn total(&self) -> f64 {
        self.inner.total()
    }

    fn format_history(&self, markdown: bool, verbose: Verbosity) -> String {
        format!("({})", self.inner.format_history(markdown, verbose))
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

impl<Dice: DiceKind> Rollable for RollSpec<Dice> {
    type Roll = Result<EvaluatedRollSpec<Dice>>;

    fn roll_with_source(&self, rng: &mut dyn DiceRollSource) -> Result<EvaluatedRollSpec<Dice>> {
        self.dyn_roll(rng)
    }
}

pub trait EvaluatedExpression {
    fn total(&self) -> f64;
    fn format_history(&self, markdown: bool, verbose: Verbosity) -> String;

    fn format(&self, markdown: bool, verbose: Verbosity) -> String {
        let history = self.format_history(markdown, verbose);
        let total = self.total();
        if markdown {
            format!("{history} = **{total}**")
        } else {
            format!("{history} = {total}",)
        }
    }
}

struct EvaluatedRollSpec<Dice: DiceKind> {
    total: i64,
    /// All modifications applied to the batch of rolls. Empty of none.
    history: Vec<(RollBatchModifier<Dice::Roll>, ModifiedRollBatch<Dice::Roll>)>,
    /// The final dice, after apply all modifications.
    ///
    /// Same as `.after()` for last entry in history (when history is not empty).
    final_rolls: RollBatch<Dice>,
}

#[derive(Clone, Copy)]
pub enum Verbosity {
    Short,
    Medium,
    Verbose,
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
                    format_rolls(original),
                    format_rolls(self.final_rolls.rolls.iter())
                )
            } else {
                let mut stages = vec![];
                for s in &self.history {
                    let rolls = format_rolls(s.1.rolls.iter().map(|m| m.format(markdown)));
                    let stage = format!("{}{}", rolls, s.0);
                    stages.push(stage);
                }

                if matches!(verbose, Verbosity::Verbose) {
                    stages.push(format_rolls(self.final_rolls.rolls.iter()));
                }

                stages.join(" 🡲 ")
            }
        } else {
            format_rolls(self.final_rolls.rolls.iter())
        }
    }
}

fn format_rolls<I: Iterator>(rolls: I) -> String
where
    I::Item: Display,
{
    format!("[{}]", format_join(rolls, ", "))
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

/// Parse a single (non-repeated) dice expression.
fn parse_single_command(s: &str) -> Result<Expression> {
    let expr = {
        let mut pairs = RollParser::parse(Rule::single_command, &s)?;
        let expr_type = pairs.next().unwrap();
        assert_eq!(expr_type.as_rule(), Rule::expr);
        expr_type.into_inner()
    };

    let roll_res = parse_expression(expr)?;
    Ok(roll_res)
}

fn parse_expression(expr: Pairs<Rule>) -> Result<Expression> {
    get_climber().climb(
        expr,
        |pair: Pair<Rule>| {
            Ok(match pair.as_rule() {
                Rule::integer => {
                    Expression::new(pair.as_str().replace(' ', "").parse::<i64>().unwrap())
                }
                Rule::float => {
                    Expression::new(pair.as_str().replace(' ', "").parse::<f64>().unwrap())
                }
                Rule::block_expr => {
                    let expr = pair.into_inner().next().unwrap().into_inner();
                    Expression::new(BlockExpression {
                        inner: parse_expression(expr)?,
                    })
                }
                Rule::dice => {
                    let expr = pair.into_inner();
                    Expression::new(parse_dice(expr)?)
                }
                _ => unreachable!("{:#?}", pair),
            })
        },
        |lhs: Result<Expression>, op: Pair<Rule>, rhs: Result<Expression>| match (lhs, rhs) {
            (Ok(left), Ok(right)) => Ok(match op.as_rule() {
                Rule::add => Expression::new(BinaryExpression {
                    left,
                    op: BinaryOp::Add,
                    right,
                }),
                Rule::sub => Expression::new(BinaryExpression {
                    left,
                    op: BinaryOp::Sub,
                    right,
                }),
                Rule::mul => Expression::new(BinaryExpression {
                    left,
                    op: BinaryOp::Mul,
                    right,
                }),
                Rule::div => Expression::new(BinaryExpression {
                    left,
                    op: BinaryOp::Div,
                    right,
                }),
                _ => unreachable!(),
            }),
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e),
        },
    )
}

fn extract_option_value(option: Pair<Rule>) -> Option<u32> {
    option
        .into_inner()
        .next()
        .map(|p| p.as_str().parse::<u32>().unwrap())
}

fn parse_dice(mut dice: Pairs<Rule>) -> Result<RollSpec<NonZeroU32>> {
    let number_of_dice = dice.next().unwrap();
    let number_of_dice = match number_of_dice.as_rule() {
        Rule::number_of_dice => {
            dice.next(); // skip `d` token
            number_of_dice.as_str().parse::<usize>().unwrap() // TODO: proper error
        }
        Rule::roll => 1, // no number before `d`, assume 1 dice
        _ => unreachable!("{:?}", number_of_dice),
    };

    let pair = dice.next().unwrap();
    let dice_parsed = match pair.as_rule() {
        Rule::number => pair.as_str().parse::<NonZeroU32>().unwrap(),
        //TODO:  Rule::fudge => (6, true),
        _ => unreachable!("{:?}", pair),
    };

    let sides: u32 = DiceKind::max(&dice_parsed).into();

    let mut modifiers: Vec<RollBatchModifier<u32>> = vec![];

    let mut aggregator: Aggregator<u32> = Aggregator::Sum;
    let mut next_option = dice.next();

    while next_option.is_some() {
        let option = next_option.unwrap();

        match &option.as_rule() {
            Rule::explode => {
                let value = extract_option_value(option).unwrap_or(sides);
                modifiers.push(RollBatchModifier::PerRollModifier(
                    PerRollModifier::ExplodeOnce(value),
                ));
            }
            Rule::i_explode => {
                let value = extract_option_value(option).unwrap_or(sides);
                modifiers.push(RollBatchModifier::PerRollModifier(
                    PerRollModifier::ExplodeUnlimited(value),
                ));
            }
            Rule::reroll => {
                let value = extract_option_value(option).unwrap();
                modifiers.push(RollBatchModifier::PerRollModifier(
                    PerRollModifier::RerollOnce(value),
                ));
            }
            Rule::i_reroll => {
                let value = extract_option_value(option).unwrap();
                modifiers.push(RollBatchModifier::PerRollModifier(
                    PerRollModifier::RerollUnlimited(value),
                ));
            }
            Rule::keep_hi => {
                let value = extract_option_value(option).unwrap();
                modifiers.push(RollBatchModifier::KeepOrDrop(KeepOrDrop::KeepHi(
                    usize::try_from(value).unwrap(),
                )));
            }
            Rule::keep_lo => {
                let value = extract_option_value(option).unwrap();
                modifiers.push(RollBatchModifier::KeepOrDrop(KeepOrDrop::KeepLo(
                    usize::try_from(value).unwrap(),
                )));
            }
            Rule::drop_hi => {
                let value = extract_option_value(option).unwrap();
                modifiers.push(RollBatchModifier::KeepOrDrop(KeepOrDrop::DropHi(
                    usize::try_from(value).unwrap(),
                )));
            }
            Rule::drop_lo => {
                let value = extract_option_value(option).unwrap();
                modifiers.push(RollBatchModifier::KeepOrDrop(KeepOrDrop::DropLo(
                    usize::try_from(value).unwrap(),
                )));
            }
            Rule::target => {
                let value_or_enum = option.into_inner().next().unwrap();
                match value_or_enum.as_rule() {
                    Rule::number => {
                        let value = value_or_enum.as_str().parse::<u32>().unwrap();
                        let (target, fail) = match aggregator {
                            Aggregator::TargetFailureDouble(t, f, 0) => (t, f),
                            Aggregator::Sum => (sides + 1, 0),
                            _ => Err("Invalid: targets")?,
                        };
                        aggregator = Aggregator::TargetFailureDouble(target, fail, value)
                    }

                    Rule::target_enum => {
                        let numbers_list = value_or_enum.into_inner();
                        let numbers_list: Vec<_> = numbers_list
                            .map(|p| p.as_str().parse::<u32>().unwrap())
                            .collect();
                        aggregator =
                            Aggregator::TargetEnum(HashSet::from_iter(numbers_list.into_iter()))
                    }
                    _ => unreachable!(),
                };
            }
            Rule::double_target => {
                let value = extract_option_value(option).unwrap();
                let (target, fail) = match aggregator {
                    Aggregator::TargetFailureDouble(t, f, 0) => (t, f),
                    Aggregator::Sum => (value, 0),
                    _ => Err("Invalid: targets")?,
                };
                aggregator = Aggregator::TargetFailureDouble(target, fail, value)
            }
            Rule::failure => {
                let value = extract_option_value(option).unwrap();
                let (target, double_target) = match aggregator {
                    Aggregator::TargetFailureDouble(t, 0, d) => (t, d),
                    Aggregator::Sum => (0, sides + 1),
                    _ => Err("Invalid: targets")?,
                };
                aggregator = Aggregator::TargetFailureDouble(target, value, double_target)
            }
            _ => unreachable!("{:#?}", option),
        }

        next_option = dice.next();
    }

    Ok(RollSpec {
        dice: dice_parsed,
        number_of_dice,
        modifiers,
        aggregator,
    })
}

#[cfg(test)]
mod tests {
    use crate::tests::IteratorDiceRollSource;

    use super::*;

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
            "[1, 2, 3, 4] 🡲 [5, 4, 6]"
        );
        assert_eq!(
            result.format_history(false, Verbosity::Medium),
            "[Drop(1), Drop(2), 3, 4]K2 🡲 [3(Exploded)🡵5, 4(Exploded)🡵6]e1 🡲 [Drop(3), 5, 4, 6]d1"
        );
        assert_eq!(
            result.format_history(true, Verbosity::Medium),
            "[~~*1*~~, ~~*2*~~, 3, 4]K2 🡲 [**3**🡵5, **4**🡵6]e1 🡲 [~~*3*~~, 5, 4, 6]d1"
        );

        assert_eq!(
            result.format_history(false, Verbosity::Verbose),
            "[Drop(1), Drop(2), 3, 4]K2 🡲 [3(Exploded)🡵5, 4(Exploded)🡵6]e1 🡲 [Drop(3), 5, 4, 6]d1 🡲 [5, 4, 6]"
        );
        assert_eq!(
            result.format_history(true, Verbosity::Verbose),
            "[~~*1*~~, ~~*2*~~, 3, 4]K2 🡲 [**3**🡵5, **4**🡵6]e1 🡲 [~~*3*~~, 5, 4, 6]d1 🡲 [5, 4, 6]"
        );
    }

    #[test]
    fn dice_command_sum() {
        let spec = parse_single_command("2d20 e2").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..21).chain(Some(20)),
            })
            .unwrap();
        assert_eq!(
            result.format_history(true, Verbosity::Medium),
            "[1, **2**🡵3]e2"
        );

        assert_eq!(result.total(), 6.0);
    }

    #[test]
    fn dice_command_check() {
        let spec = parse_single_command("20d20 e tt20").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..21).chain(Some(20)),
            })
            .unwrap();
        assert_eq!(
            result.format_history(true, Verbosity::Medium),
            "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, **20**🡵20]e20"
        );

        assert_eq!(result.total(), 4.0);
    }

    #[test]
    fn single_command() {
        let spec = parse_single_command("1 + 2 * 3 + 1d1 e1").unwrap();
        let result = spec.roll().unwrap();
        assert_eq!(
            result.format_history(true, Verbosity::Medium),
            "1 + 2 * 3 + [**1**🡵1]e1"
        );

        assert_eq!(result.total(), 9.0);
    }

    #[test]
    fn single_command_blocks() {
        let spec = Expression::parse("1 + 2 * (3 + 1d1 e1)").unwrap();
        let result = spec.roll().unwrap();
        assert_eq!(
            result.format(true, Verbosity::Medium),
            "1 + 2 * (3 + [**1**🡵1]e1) = **11**"
        );

        assert_eq!(result.total(), 11.0);
    }
}
