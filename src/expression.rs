use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    rc::Rc,
};

use pest::iterators::{Pair, Pairs};

use crate::{
    DiceRollSource, Result, RollError, Rollable,
    dice_expression::parse_dice,
    dice_kind::basic::BasicDice,
    parser::{Rule, climb},
};

/// A parsed dice expression.
#[derive(Clone, Debug)]
pub struct Expression(Rc<dyn ExpressionRollable>);

impl Display for Expression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&*self.0, f)
    }
}

pub type ExpressionResult = Result<Box<dyn EvaluatedExpression>>;

pub(crate) trait ExpressionRollable: Debug + Display {
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

    fn format<T: Display>(&self, left: T, right: T, markdown: bool) -> String {
        format!(
            "{left}{}{right}",
            match self {
                BinaryOp::Add => " + ",
                BinaryOp::Sub => " - ",
                BinaryOp::Mul =>
                    if markdown {
                        r"\*"
                    } else {
                        "*"
                    },
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

impl Display for BinaryExpression<Expression> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.op.format(&self.left, &self.right, false))
    }
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
            markdown,
        )
    }
}

#[derive(Debug, Clone)]

struct RollableFloat(f64);

impl ExpressionRollable for RollableFloat {
    fn expression_roll(&self, _rng: &mut dyn DiceRollSource) -> ExpressionResult {
        Ok(Box::new(self.clone()))
    }
}

impl Display for RollableFloat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.fract() == 0.0 {
            // Include ".0" at the end of integer values so if they round trip, its clear they are a float not an int.
            write!(f, "{:.1}", self.0)
        } else {
            write!(f, "{:.}", self.0)
        }
    }
}

impl EvaluatedExpression for RollableFloat {
    fn total(&self) -> f64 {
        self.0
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

impl Display for BlockExpression<Expression> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = &*self.inner.0;
        write!(f, "({})", inner)
    }
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

#[derive(Debug)]
struct VariableReference {
    identifier: String,
    inner: Expression,
}

impl ExpressionRollable for VariableReference {
    fn expression_roll(&self, rng: &mut dyn DiceRollSource) -> ExpressionResult {
        Ok(Box::new(BlockExpression {
            inner: self.inner.roll_with_source(rng)?,
        }))
    }
}

impl Display for VariableReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = &*self.inner.0;
        write!(f, "(${}: {})", self.identifier, inner)
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
    /// Skips showing some intermediate steps
    Short,
    /// Shows every step
    Medium,
    /// Shows redundant summary information
    Verbose,
}

pub(crate) fn format_bold<V: Display>(value: V, markdown: bool) -> String {
    if markdown {
        format!("**{value}**")
    } else {
        format!("{value}")
    }
}

pub(crate) fn parse_expression(
    expr: Pairs<Rule>,
    variables: &HashMap<String, Expression>,
) -> Result<Expression> {
    let _ = variables;
    climb(
        expr,
        |pair: Pair<Rule>| {
            Ok(match pair.as_rule() {
                Rule::integer => Expression::new(pair.as_str().replace(' ', "").parse::<i64>()?),
                Rule::float => Expression::new(RollableFloat(
                    pair.as_str().replace(' ', "").parse::<f64>()?,
                )),
                Rule::block_expr => {
                    let expr = pair.into_inner().next().unwrap().into_inner();
                    Expression::new(BlockExpression {
                        inner: parse_expression(expr, variables)?,
                    })
                }
                Rule::dice => {
                    let expr = pair.into_inner();
                    parse_dice::<BasicDice>(expr)?
                }
                Rule::variable_identifier => {
                    let identifier = pair.as_str();
                    match variables.get(identifier) {
                        Some(expression) => Expression::new(VariableReference {
                            identifier: identifier.to_string(),
                            inner: expression.clone(),
                        }),
                        None => {
                            return Err(RollError::ParamError(format!(
                                "Reference to undefined variable \"{identifier}\""
                            )));
                        }
                    }
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
    use crate::tests::IteratorDiceRollSource;

    #[test]
    fn single_command_blocks() {
        let spec = Expression::parse("1 + 2 * (3 + 1d1 e1)").unwrap();
        let result = spec.roll().unwrap();
        assert_eq!(
            result.format(true, Verbosity::Medium),
            "1 + 2\\*(3 + \\[**1**&#x200B;🡵1\\]e1) = **11**"
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
            "\\[(-), ( ), (+)\\] = **0**"
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
            "\\[~~*(-)*~~, ( ), (+)\\]d1 = **1**"
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
            "\\[(-), ( )\\] + \\[3\\] = **2**"
        );
    }
}
