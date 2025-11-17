use std::sync::{Arc, Once, RwLock};

use pest::{
    iterators::{Pair, Pairs},
    pratt_parser::PrattParser,
};
use pest_derive::Parser;

use crate::{error::Result, DiceResult, RollError, SingleRollResult};

pub trait DiceRollSource {
    fn roll_single_die(&mut self, sides: u64) -> u64;
}

#[derive(Parser)]
#[grammar = "caith.pest"]
pub(crate) struct RollParser;

// arbitrary limit to avoid OOM
const MAX_DICE_SIDES: u64 = 5000;
const MAX_NUMBER_OF_DICE: u64 = 5000;

// number represent nb dice to keep/drop
#[derive(Clone, PartialEq)]
pub(crate) enum TotalModifier {
    KeepHi(usize),
    KeepLo(usize),
    DropHi(usize),
    DropLo(usize),
    /// These values are in order:
    /// (target (threshold for success),
    /// failure (threshold for negative success),
    /// target doubled (threshold for two successes per dice))
    TargetFailureDouble(u64, u64, u64),
    // List of specific values which count as success
    TargetEnum(Vec<u64>),
    Fudge,
    None(Rule),
}

struct OptionResult {
    res: Vec<DiceResult>,
    modifier: TotalModifier,
}

// Struct to have a singleton of PrecClimber without using once_cell
#[derive(Clone)]
pub(crate) struct Climber {
    inner: Arc<RwLock<PrattParser<Rule>>>,
}

impl Climber {
    pub fn climb<'i, P, F, G, T>(&self, pairs: P, primary: F, infix: G) -> T
    where
        P: Iterator<Item = Pair<'i, Rule>>,
        F: FnMut(Pair<'i, Rule>) -> T,
        G: FnMut(T, Pair<'i, Rule>, T) -> T + 'i,
    {
        self.inner
            .read()
            .unwrap()
            .map_primary(primary)
            .map_infix(infix)
            .parse(pairs)
    }
}

pub(crate) fn get_climber() -> Climber {
    static mut PREC_CLIMBER: *const Climber = 0 as *const Climber;
    static ONCE: Once = Once::new();

    unsafe {
        ONCE.call_once(|| {
            use pest::pratt_parser::{Assoc, Op};

            // Make it
            let singleton = Climber {
                inner: Arc::new(RwLock::new(
                    PrattParser::new()
                        .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::sub, Assoc::Left))
                        .op(Op::infix(Rule::mul, Assoc::Left) | Op::infix(Rule::div, Assoc::Left)),
                )),
            };

            // Put it in the heap so it can outlive this call
            PREC_CLIMBER = std::mem::transmute(Box::new(singleton));
        });

        // Now we give out a copy of the data that is safe to use concurrently.
        (*PREC_CLIMBER).clone()
    }
}

fn compute_explode(
    rolls: &mut SingleRollResult,
    sides: u64,
    res: Vec<DiceResult>,
    option: Pair<Rule>,
    prev_modifier: &TotalModifier,
    rng: &mut dyn DiceRollSource,
) -> OptionResult {
    let value = extract_option_value(option).unwrap_or(sides);
    let nb = res.iter().filter(|x| x.res >= value).count() as u64;
    if prev_modifier != &TotalModifier::None(Rule::explode)
        && prev_modifier != &TotalModifier::None(Rule::i_explode)
    {
        rolls.add_history(res.clone(), false);
    }
    let res = if nb > 0 {
        let res = roll_dice(nb, sides, rng);
        rolls.add_history(res.clone(), false);
        res
    } else {
        res
    };
    OptionResult {
        modifier: TotalModifier::None(Rule::explode),
        res,
    }
}

fn compute_i_explode(
    rolls: &mut SingleRollResult,
    sides: u64,
    res: Vec<DiceResult>,
    option: Pair<Rule>,
    prev_modifier: &TotalModifier,
    rng: &mut dyn DiceRollSource,
) -> OptionResult {
    let value = extract_option_value(option).unwrap_or(sides);
    if prev_modifier != &TotalModifier::None(Rule::explode)
        && prev_modifier != &TotalModifier::None(Rule::i_explode)
    {
        rolls.add_history(res.clone(), false);
    }
    let mut nb = res.into_iter().filter(|x| x.res >= value).count() as u64;
    let mut res = Vec::new();
    while nb > 0 {
        res = roll_dice(nb, sides, rng);
        nb = res.iter().filter(|x| x.res >= value).count() as u64;
        rolls.add_history(res.clone(), false);
    }
    OptionResult {
        modifier: TotalModifier::None(Rule::i_explode),
        res,
    }
}

fn compute_reroll(
    rolls: &mut SingleRollResult,
    sides: u64,
    res: Vec<DiceResult>,
    option: Pair<Rule>,
    rng: &mut dyn DiceRollSource,
) -> OptionResult {
    let value = extract_option_value(option).unwrap();
    let reroll = compute_reroll_inner(rolls, sides, &res, value, rng);

    OptionResult {
        modifier: TotalModifier::None(Rule::reroll),
        res: reroll.unwrap_or(res),
    }
}

fn compute_reroll_inner(
    rolls: &mut SingleRollResult,
    sides: u64,
    res: &Vec<DiceResult>,
    value: u64,
    rng: &mut dyn DiceRollSource,
) -> Option<Vec<DiceResult>> {
    let mut has_rerolled = false;
    let mut rerolls: Vec<Vec<DiceResult>> = vec![];
    let res_new: Vec<DiceResult> = res
        .iter()
        .map(|x| {
            let mut inner = vec![*x];
            let result = if x.res <= value {
                has_rerolled = true;
                let rerolled = roll_dice(1, sides, rng)[0];
                inner.push(rerolled);
                rerolled
            } else {
                *x
            };
            rerolls.push(inner);
            result
        })
        .collect();

    if has_rerolled {
        rolls.add_rerolled_history(rerolls);
        return Some(res_new);
    }
    None
}

fn compute_i_reroll(
    rolls: &mut SingleRollResult,
    sides: u64,
    mut res: Vec<DiceResult>,
    option: Pair<Rule>,
    rng: &mut dyn DiceRollSource,
) -> Result<OptionResult> {
    let value = extract_option_value(option).unwrap();
    if value >= sides {
        return Err(RollError::ParamError(
            format!("Cannot infinitely reroll dice of {value} or lower since the maximum roll is {sides}: this would go on forever")
        ));
    }
    loop {
        let reroll = compute_reroll_inner(rolls, sides, &res, value, rng);
        match reroll {
            Some(r) => res = r,
            None => {
                return Ok(OptionResult {
                    modifier: TotalModifier::None(Rule::i_reroll),
                    res,
                })
            }
        }
    }
}

fn compute_option(
    mut rolls: &mut SingleRollResult,
    sides: u64,
    res: Vec<DiceResult>,
    option: Pair<Rule>,
    rng: &mut dyn DiceRollSource,
    prev_modifier: &TotalModifier,
) -> Result<OptionResult> {
    fn keep_or_drop(
        rolls: &mut SingleRollResult,
        res: &Vec<DiceResult>,
        modifier: TotalModifier,
    ) -> Result<OptionResult> {
        // TODO: why is this logic duplicated here and in compute_total
        let flagged = apply_total_modifier(&modifier, &res, |r| r.res)?;
        let out: Vec<DiceResult> = flagged
            .iter()
            .filter(|(f, _)| *f)
            .map(|(_, r)| r.clone())
            .collect();
        if res.len() != out.len() {
            rolls.add_discard_history(flagged);
        }
        Ok(OptionResult { res: out, modifier })
    }

    match &option.as_rule() {
        Rule::explode => Ok(compute_explode(
            rolls,
            sides,
            res,
            option,
            prev_modifier,
            rng,
        )),
        Rule::i_explode => Ok(compute_i_explode(
            rolls,
            sides,
            res,
            option,
            prev_modifier,
            rng,
        )),
        Rule::reroll => Ok(compute_reroll(rolls, sides, res, option, rng)),
        Rule::i_reroll => compute_i_reroll(rolls, sides, res, option, rng),
        Rule::keep_hi => {
            let value = extract_option_value(option).unwrap();
            keep_or_drop(&mut rolls, &res, TotalModifier::KeepHi(value as usize))
        }
        Rule::keep_lo => {
            let value = extract_option_value(option).unwrap();
            keep_or_drop(&mut rolls, &res, TotalModifier::KeepLo(value as usize))
        }
        Rule::drop_hi => {
            let value = extract_option_value(option).unwrap();
            keep_or_drop(&mut rolls, &res, TotalModifier::DropHi(value as usize))
        }
        Rule::drop_lo => {
            let value = extract_option_value(option).unwrap();
            keep_or_drop(&mut rolls, &res, TotalModifier::DropLo(value as usize))
        }
        Rule::target => {
            let value_or_enum = option.into_inner().next().unwrap();
            let modifier = match value_or_enum.as_rule() {
                Rule::number => TotalModifier::TargetFailureDouble(
                    value_or_enum.as_str().parse::<u64>().unwrap(),
                    0,
                    0,
                ),

                Rule::target_enum => {
                    let numbers_list = value_or_enum.into_inner();
                    let numbers_list: Vec<_> = numbers_list
                        .map(|p| p.as_str().parse::<u64>().unwrap())
                        .collect();
                    TotalModifier::TargetEnum(numbers_list)
                }
                _ => unreachable!(),
            };
            Ok(OptionResult { res, modifier })
        }
        Rule::double_target => {
            let value = extract_option_value(option).unwrap();
            let modifier = TotalModifier::TargetFailureDouble(0, 0, value);
            Ok(OptionResult { res, modifier })
        }
        Rule::failure => {
            let value = extract_option_value(option).unwrap();
            let modifier = TotalModifier::TargetFailureDouble(0, value, 0);
            Ok(OptionResult { res, modifier })
        }
        _ => unreachable!("{:#?}", option),
    }
}

pub(crate) fn apply_total_modifier<T: Clone>(
    modifier: &TotalModifier,
    v: &[T],
    get_number: impl Fn(&T) -> u64,
) -> Result<Vec<(bool, T)>> {
    let res = match modifier {
        TotalModifier::KeepHi(n) => keep_low(&v, *n, |result| u64::MAX - get_number(&result))?,
        TotalModifier::KeepLo(n) => keep_low(&v, *n, |result| get_number(&result))?,
        TotalModifier::DropHi(n) => keep_low(&v, v.len() - n, |result| get_number(&result))?,
        TotalModifier::DropLo(n) => {
            keep_low(&v, v.len() - n, |result| u64::MAX - get_number(&result))?
        }
        TotalModifier::None(_)
        | TotalModifier::TargetFailureDouble(_, _, _)
        | TotalModifier::TargetEnum(_)
        | TotalModifier::Fudge => v.iter().map(|f| (true, f.clone())).collect(),
    };
    Ok(res)
}

/// Copy `v`, but with the top (as defined by `f`) `to_drop` entries flagged with false and the rest with true.
pub(crate) fn keep_low<T: Clone, Key: Ord + Copy>(
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
    use crate::parser::keep_low;

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
}

fn compute_roll(mut dice: Pairs<Rule>, rng: &mut dyn DiceRollSource) -> Result<SingleRollResult> {
    let mut rolls = SingleRollResult::new();
    let number_of_dice = dice.next().unwrap();
    let number_of_dice = match number_of_dice.as_rule() {
        Rule::number_of_dice => {
            dice.next(); // skip `d` token
            let n = number_of_dice.as_str().parse::<u64>().unwrap();
            if n > MAX_NUMBER_OF_DICE {
                return Err(format!(
                    "Exceed maximum allowed number of dices ({})",
                    MAX_NUMBER_OF_DICE
                )
                .into());
            }
            n
        }
        Rule::roll => 1, // no number before `d`, assume 1 dice
        _ => unreachable!("{:?}", number_of_dice),
    };

    let pair = dice.next().unwrap();
    let (sides, is_fudge) = match pair.as_rule() {
        Rule::number => (pair.as_str().parse::<u64>().unwrap(), false),
        Rule::fudge => (6, true),
        _ => unreachable!("{:?}", pair),
    };

    if sides == 0 {
        return Err("Dice can't have 0 sides".into());
    } else if sides > MAX_DICE_SIDES {
        return Err(format!("Dice can't have more than {}", MAX_DICE_SIDES).into());
    }

    let mut res = roll_dice(number_of_dice, sides, rng);
    let mut modifier = TotalModifier::None(Rule::expr);
    let mut next_option = dice.next();
    if !is_fudge {
        while next_option.is_some() {
            let option = next_option.unwrap();
            let opt_res = compute_option(&mut rolls, sides, res, option, rng, &modifier)?;
            res = opt_res.res;
            modifier = match opt_res.modifier {
                TotalModifier::TargetFailureDouble(t, f, d) => match modifier {
                    TotalModifier::TargetFailureDouble(ot, of, od) => {
                        if t > 0 {
                            TotalModifier::TargetFailureDouble(t, of, od)
                        } else if f > 0 {
                            TotalModifier::TargetFailureDouble(ot, f, od)
                        } else {
                            TotalModifier::TargetFailureDouble(ot, of, d)
                        }
                    }
                    _ => opt_res.modifier,
                },
                TotalModifier::TargetEnum(_) => opt_res.modifier,
                _ => opt_res.modifier,
            };
            next_option = dice.next();
        }
    } else {
        modifier = TotalModifier::Fudge;
    }
    rolls.add_history(res, is_fudge);
    rolls.compute_total(modifier)?;

    Ok(rolls)
}

/// compute a whole roll expression
pub(crate) fn compute(
    expr: Pairs<Rule>,
    rng: &mut dyn DiceRollSource,
    is_block: bool,
) -> Result<SingleRollResult> {
    let res = get_climber().climb(
        expr,
        |pair: Pair<Rule>| match pair.as_rule() {
            Rule::integer => Ok(SingleRollResult::with_total(
                pair.as_str().replace(' ', "").parse::<i64>().unwrap(),
            )),
            Rule::float => Ok(SingleRollResult::with_float(
                pair.as_str().replace(' ', "").parse::<f64>().unwrap(),
            )),
            Rule::block_expr => {
                let expr = pair.into_inner().next().unwrap().into_inner();
                compute(expr, rng, true)
            }
            Rule::dice => compute_roll(pair.into_inner(), rng),
            _ => unreachable!("{:#?}", pair),
        },
        |lhs: Result<SingleRollResult>, op: Pair<Rule>, rhs: Result<SingleRollResult>| match (
            lhs, rhs,
        ) {
            (Ok(lhs), Ok(rhs)) => match op.as_rule() {
                Rule::add => Ok(lhs + rhs),
                Rule::sub => Ok(lhs - rhs),
                Rule::mul => Ok(lhs * rhs),
                Rule::div => {
                    if rhs.is_zero() {
                        Err("Can't divide by zero".into())
                    } else {
                        Ok(lhs / rhs)
                    }
                }
                _ => unreachable!(),
            },
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e),
        },
    );
    match res {
        Ok(mut single_roll_res) => {
            if is_block {
                single_roll_res.add_parenthesis();
            }
            Ok(single_roll_res)
        }
        e @ Err(_) => e,
    }
}

pub(crate) fn find_first_dice(expr: &mut Pairs<Rule>) -> Option<String> {
    let mut next_pair = expr.next();
    while next_pair.is_some() {
        let pair = next_pair.unwrap();
        match pair.as_rule() {
            Rule::expr => return find_first_dice(&mut pair.into_inner()),
            Rule::dice => return Some(pair.as_str().trim().to_owned()),
            _ => (),
        };
        next_pair = expr.next();
    }
    None
}

pub(crate) fn roll_dice(num: u64, sides: u64, rng: &mut dyn DiceRollSource) -> Vec<DiceResult> {
    (0..num)
        .map(|_| DiceResult::new(rng.roll_single_die(sides), sides))
        .collect()
}

fn extract_option_value(option: Pair<Rule>) -> Option<u64> {
    option
        .into_inner()
        .next()
        .map(|p| p.as_str().parse::<u64>().unwrap())
}
