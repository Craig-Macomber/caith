use std::sync::LazyLock;

use pest::{iterators::Pair, pratt_parser::PrattParser};
use pest_derive::Parser;

use crate::error::Result;

pub trait DiceRollSource {
    fn roll_single_die(&mut self, sides: u64) -> u64;
}

#[derive(Parser)]
#[grammar = "caith.pest"]
pub(crate) struct RollParser;

pub(crate) fn climb<'i, P, F, G, T>(pairs: P, primary: F, infix: G) -> T
where
    P: Iterator<Item = Pair<'i, Rule>>,
    F: FnMut(Pair<'i, Rule>) -> T,
    G: FnMut(T, Pair<'i, Rule>, T) -> T + 'i,
{
    static PARSER: LazyLock<PrattParser<Rule>> = LazyLock::new(|| {
        use pest::pratt_parser::{Assoc, Op};
        PrattParser::new()
            .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::sub, Assoc::Left))
            .op(Op::infix(Rule::mul, Assoc::Left) | Op::infix(Rule::div, Assoc::Left))
    });
    PARSER.map_primary(primary).map_infix(infix).parse(pairs)
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
