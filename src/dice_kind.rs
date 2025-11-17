use std::{
    fmt::{Debug, Display},
    hash::Hash,
    num::NonZeroU32,
    rc::Rc,
    str::FromStr,
};

use pest::{
    iterators::{Pair, Pairs},
    Parser,
};

use crate::{
    dice_expression::parse_dice,
    parser::{get_climber, DiceRollSource, RollParser, Rule},
    Result, Rollable,
};

/// A kind of dice which can be rolled.
pub(crate) trait DiceKind: Copy + FromStr<Err: Debug> + 'static + Debug {
    type Roll: Roll;
    fn roll(&self, rng: &mut dyn DiceRollSource) -> Self::Roll;
    fn max(&self) -> Self::Roll;
    fn min(&self) -> Self::Roll;
}

pub(crate) trait Roll:
    Ord + Into<i64> + Copy + Hash + Display + FromStr<Err: Debug> + Debug
{
}

/// A parsed dice expression.
#[derive(Clone, Debug)]
pub struct Expression(Rc<dyn ExpressionRollable>);

pub type ExpressionResult = Result<Box<dyn EvaluatedExpression>>;

pub(crate) trait ExpressionRollable: Debug {
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

    pub(crate) fn new<T: ExpressionRollable + 'static>(expression: T) -> Expression {
        Expression(Rc::new(expression))
    }
}

#[derive(Clone, Copy, Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

/// Result of evaluating an [Expression].
pub trait EvaluatedExpression: Debug {
    /// Numeric result.
    /// Unless division or floats are involved, this will be an integer.
    fn total(&self) -> f64;

    /// Pretty print the rolls and adjustments to them which produced the result.
    fn format_history(&self, markdown: bool, verbose: Verbosity) -> String;

    /// Format history and total into one string.
    fn format(&self, markdown: bool, verbose: Verbosity) -> String {
        let history = self.format_history(markdown, verbose);
        let total = format_bold(self.total(), markdown);
        format!("{history} = {total}",)
    }
}

/// A verbosity level for formatting output.
#[derive(Clone, Copy)]
pub enum Verbosity {
    Short,
    Medium,
    Verbose,
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

/// A parsed command.
#[derive(Debug)]
pub struct Command {
    expression: Expression,
    repeat: Option<RepeatedCommand>,
    reason: Option<String>,
}

impl Rollable for Command {
    type Roll = Result<EvaluatedCommand>;

    fn roll_with_source(&self, rng: &mut dyn DiceRollSource) -> Self::Roll {
        let count: usize = self.repeat.as_ref().map(|r| r.count).unwrap_or(1);
        let expressions: Result<Vec<Box<dyn EvaluatedExpression>>> = (0..count as isize)
            .map(|_i| self.expression.roll_with_source(rng))
            .collect();
        let mut expressions = expressions?;

        let total: Option<f64> = match self.repeat {
            Some(repeat) => match repeat.mode {
                RepeatedMode::Sum => Some(expressions.iter().fold(0.0, |x, y| x + y.total())),
                RepeatedMode::Sort => {
                    expressions.sort_by(|a, b| f64::total_cmp(&a.total(), &b.total()));
                    None
                }
                RepeatedMode::None => None,
            },
            None => Some(expressions.first().unwrap().total()),
        };

        let repeat: Option<RepeatedCommand> = self.repeat;
        let reason: Option<String> = self.reason.clone();

        Ok(EvaluatedCommand {
            total,
            expressions,
            repeat,
            reason,
        })
    }
}

#[derive(Debug)]
pub struct EvaluatedCommand {
    total: Option<f64>,
    expressions: Vec<Box<dyn EvaluatedExpression>>,
    repeat: Option<RepeatedCommand>,
    reason: Option<String>,
}

fn format_bold<V: Display>(value: V, markdown: bool) -> String {
    if markdown {
        format!("**{value}**")
    } else {
        format!("{value}")
    }
}

impl Display for EvaluatedCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format(false, Verbosity::Medium))
    }
}

impl Display for dyn EvaluatedExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format(false, Verbosity::Medium))
    }
}

impl EvaluatedCommand {
    /// If this command is a single (non-repeated) expression, OR a summed repeated expression, this gives the total.
    /// Otherwise there is no total, and [None] is returned.
    pub fn total(&self) -> Option<f64> {
        self.total
    }

    /// Pretty print the entire command results, including history and total (if appropriate).
    pub fn format(&self, markdown: bool, verbose: Verbosity) -> String {
        let inner: Vec<String> = self
            .expressions
            .iter()
            .map(|x| x.format(markdown, verbose))
            .collect();
        let s = match &self.repeat {
            Some(repeat) => match repeat.mode {
                RepeatedMode::Sum => format!(
                    "{} = {}",
                    inner
                        .iter()
                        .map(|s| format!("({s})"))
                        .collect::<Vec<_>>()
                        .join(" + "),
                    format_bold(self.total.unwrap(), markdown)
                ),
                RepeatedMode::Sort | RepeatedMode::None => inner
                    .iter()
                    .map(|s| format!("({s})"))
                    .collect::<Vec<_>>()
                    .join(" "),
            },
            None => inner.first().unwrap().clone(),
        };
        match &self.reason {
            Some(reason) => format!("{s} : {reason}"),
            None => s,
        }
    }

    pub fn results(&self) -> &Vec<Box<dyn EvaluatedExpression>> {
        &self.expressions
    }
}

impl Command {
    /// Parse a command expression.
    pub fn parse(s: &str) -> Result<Command> {
        let mut pairs = RollParser::parse(Rule::command, s)?;
        let expr_type = pairs.next().unwrap();
        let mut command = match expr_type.as_rule() {
            Rule::expr => Command {
                expression: parse_expression(expr_type.into_inner())?,
                repeat: None,
                reason: None,
            },
            Rule::repeated_expr => process_repeated_expr(expr_type)?,
            _ => unreachable!(),
        };

        if let Some(reason) = pairs.next() {
            if reason.as_rule() == Rule::reason {
                command.reason = Some(reason.as_str()[1..].trim().to_owned());
            }
        }
        Ok(command)
    }
}

#[derive(Clone, Copy, Debug)]
struct RepeatedCommand {
    count: usize,
    mode: RepeatedMode,
}

#[derive(Clone, Copy, Debug)]
enum RepeatedMode {
    Sum,
    Sort,
    None,
}

fn process_repeated_expr(expr_type: Pair<Rule>) -> Result<Command> {
    let mut pairs = expr_type.into_inner();
    let expr = pairs.next().unwrap();
    let maybe_option = pairs.next().unwrap();
    let (count, mode) = match maybe_option.as_rule() {
        Rule::number => (
            maybe_option.as_str().parse::<usize>().unwrap(),
            RepeatedMode::None,
        ),
        Rule::add => (
            pairs.next().unwrap().as_str().parse::<usize>().unwrap(),
            RepeatedMode::Sum,
        ),
        Rule::sort => (
            pairs.next().unwrap().as_str().parse::<usize>().unwrap(),
            RepeatedMode::Sort,
        ),
        _ => unreachable!(),
    };
    if count <= 0 {
        Err("Can't repeat 0 times or negatively".into())
    } else {
        let c = parse_expression(expr.clone().into_inner())?;
        Ok(Command {
            expression: c,
            repeat: Some(RepeatedCommand { count, mode }),
            reason: None,
        })
    }
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
                    parse_dice::<NonZeroU32>(expr)?
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tests::IteratorDiceRollSource, RollError};

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

    #[test]
    fn fudge_minimal() {
        let spec = Expression::parse("3dF").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..10),
            })
            .unwrap();
        assert_eq!(
            result.format(true, Verbosity::Medium),
            "[(-), ( ), (+)] = **0**"
        );
    }

    #[test]
    fn fudge() {
        let spec = Expression::parse("3dF d1").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..10),
            })
            .unwrap();
        assert_eq!(
            result.format(true, Verbosity::Medium),
            "[~~*(-)*~~, ( ), (+)]d1 = **1**"
        );
    }

    #[test]
    fn mixed() {
        let spec = Expression::parse("2dF + 1d6").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..10),
            })
            .unwrap();
        assert_eq!(
            result.format(true, Verbosity::Medium),
            "[(-), ( )] + [3] = **2**"
        );
    }

    #[test]
    fn command_single() {
        let spec = Command::parse("1d6").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..10),
            })
            .unwrap();
        assert_eq!(result.format(false, Verbosity::Medium), "[1] = 1");
    }

    #[test]
    fn command_repeated() {
        let spec = Command::parse("(1d6) ^ 2").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..10),
            })
            .unwrap();
        assert_eq!(
            result.format(false, Verbosity::Medium),
            "([1] = 1) ([2] = 2)"
        );
    }

    #[test]
    fn command_repeated_sum() {
        let spec = Command::parse("(1d6) ^+ 2").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..10),
            })
            .unwrap();
        assert_eq!(
            result.format(false, Verbosity::Medium),
            "([1] = 1) + ([2] = 2) = 3"
        );
    }

    #[test]
    fn command_repeated_sort() {
        let spec = Command::parse("(1d6) ^# 2").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..6).rev(),
            })
            .unwrap();
        assert_eq!(
            result.format(false, Verbosity::Medium),
            "([4] = 4) ([5] = 5)"
        );
    }

    #[test]
    fn invalid_reroll_fudge() {
        let spec = Command::parse("1dF ir6").unwrap_err();
        match spec {
            RollError::ParseError(e) => {
                assert_eq!(format!("{e}"), "number too large to fit in target type")
            }
            _ => assert!(false),
        }
    }
}
