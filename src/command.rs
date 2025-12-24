use super::{EvaluatedExpression, Expression};
use crate::{
    DiceRollSource, Result, Rollable, Verbosity,
    dice_expression::limit_dice,
    expression::{format_bold, parse_expression},
    parser::{RollParser, Rule},
};
use pest::{Parser, iterators::Pair};
use std::{collections::HashMap, fmt::Display};

/// Parse a single (non-repeated) dice expression.
pub(crate) fn parse_single_command(
    s: &str,
    variables: &HashMap<String, Expression>,
) -> Result<Expression> {
    let expr = {
        let mut pairs = RollParser::parse(Rule::single_command, s)?;
        let expr_type = pairs.next().unwrap();
        assert_eq!(expr_type.as_rule(), Rule::expr);
        expr_type.into_inner()
    };

    let roll_res = parse_expression(expr, variables)?;
    Ok(roll_res)
}

/// A parsed command.
#[derive(Debug)]
pub struct Command {
    expression: Expression,
    repeat: Option<RepeatedCommand>,
    reason: Option<String>,
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.expression)
    }
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

/// Result of rolling a [Command].
#[derive(Debug)]
pub struct EvaluatedCommand {
    total: Option<f64>,
    expressions: Vec<Box<dyn EvaluatedExpression>>,
    repeat: Option<RepeatedCommand>,
    reason: Option<String>,
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

    /// Results from each run of the expression
    pub fn results(&self) -> &Vec<Box<dyn EvaluatedExpression>> {
        &self.expressions
    }
}

impl Command {
    /// Parse a command expression.
    pub fn parse(s: &str) -> Result<Command> {
        Command::parse_with_variables(s, &HashMap::default())
    }

    /// Parse a command expression.
    pub fn parse_with_variables(
        s: &str,
        variables: &HashMap<String, Expression>,
    ) -> Result<Command> {
        let mut pairs = RollParser::parse(Rule::command, s)?;
        let expr_type = pairs.next().unwrap();
        let mut command = match expr_type.as_rule() {
            Rule::expr => Command {
                expression: parse_expression(expr_type.into_inner(), variables)?,
                repeat: None,
                reason: None,
            },
            Rule::repeated_expr => process_repeated_expr(expr_type, variables)?,
            _ => unreachable!(),
        };

        if let Some(reason) = pairs.next()
            && reason.as_rule() == Rule::reason
        {
            command.reason = Some(reason.as_str()[1..].trim().to_owned());
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

fn process_repeated_expr(
    expr_type: Pair<Rule>,
    variables: &HashMap<String, Expression>,
) -> Result<Command> {
    let mut pairs = expr_type.into_inner();
    let expr = pairs.next().unwrap();
    let maybe_option = pairs.next().unwrap();
    let (count, mode) = match maybe_option.as_rule() {
        Rule::number => (maybe_option.as_str().parse::<usize>()?, RepeatedMode::None),
        Rule::add => (
            pairs.next().unwrap().as_str().parse::<usize>()?,
            RepeatedMode::Sum,
        ),
        Rule::sort => (
            pairs.next().unwrap().as_str().parse::<usize>()?,
            RepeatedMode::Sort,
        ),
        _ => unreachable!(),
    };
    if count == 0 {
        Err("Can't repeat 0 times or negatively".into())
    } else {
        limit_dice(count, "repeated roll count")?;
        let c = parse_expression(expr.clone().into_inner(), variables)?;
        Ok(Command {
            expression: c,
            repeat: Some(RepeatedCommand { count, mode }),
            reason: None,
        })
    }
}

impl Expression {
    /// Parse as string into an [Expression].
    pub fn parse(expression: &str) -> Result<Expression> {
        Expression::parse_with_variables(expression, &HashMap::default())
    }

    /// Parse as string into an [Expression].
    pub fn parse_with_variables(
        expression: &str,
        variables: &HashMap<String, Expression>,
    ) -> Result<Expression> {
        parse_single_command(expression, variables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RollError, tests::IteratorDiceRollSource};

    #[test]
    fn dice_command_sum() {
        let spec = Expression::parse("2d20 e2").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..21).chain(Some(20)),
            })
            .unwrap();
        assert_eq!(
            result.format_history(true, Verbosity::Medium),
            "\\[1, **2**&#x200B;🡵3\\]e2"
        );

        assert_eq!(result.total(), 6.0);
    }

    #[test]
    fn dice_command_check() {
        let spec = Expression::parse("20d20 e tt20").unwrap();
        let result = spec
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..21).chain(Some(20)),
            })
            .unwrap();
        assert_eq!(
            result.format_history(true, Verbosity::Medium),
            "\\[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, **20**&#x200B;🡵20\\]e20"
        );

        assert_eq!(result.total(), 4.0);
    }

    #[test]
    fn single_command() {
        let spec = Expression::parse("1 + 2 * 3 + 1d1 e1").unwrap();
        let result = spec.roll().unwrap();
        assert_eq!(
            result.format_history(true, Verbosity::Medium),
            "1 + 2\\*3 + \\[**1**&#x200B;🡵1\\]e1"
        );

        assert_eq!(result.total(), 9.0);
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

    #[test]
    fn fudge_in_expression() {
        _ = Command::parse("1dF + 1").unwrap().roll().unwrap();
    }

    #[test]
    fn fudge_in_expression2() {
        _ = Command::parse("(1dF + 1dF)").unwrap().roll().unwrap();
    }

    #[test]
    fn formatted_command() {
        let s = format!("{}", Command::parse("(1dF + 1dF)").unwrap());
        assert_eq!(s, "(1dF + 1dF)");
    }

    #[test]
    fn variable() {
        let mut vars: HashMap<String, Expression, _> = HashMap::default();
        vars.insert("Var".to_string(), Expression::parse("5").unwrap());
        let minimal = Expression::parse_with_variables("$Var", &vars).unwrap();
        assert_eq!(format!("{minimal}"), "($Var: 5)");
        assert_eq!(minimal.roll().unwrap().total(), 5.0);

        let mixed = Expression::parse_with_variables("1 + $Var", &vars).unwrap();
        assert_eq!(format!("{mixed}"), "1 + ($Var: 5)");
        assert_eq!(mixed.roll().unwrap().total(), 6.0);
    }
}
