use crate::error::Result;

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
                keep_low(v, *n, |result| std::cmp::Reverse(get_number(result)))?
            }
            KeepOrDrop::KeepLo(n) => keep_low(v, *n, |result| get_number(result))?,
            KeepOrDrop::DropHi(n) => keep_low(
                v,
                v.len().checked_sub(*n).ok_or_else(|| {
                    format!("Cannot drop {n} dice when there are only {}", v.len())
                })?,
                |result| get_number(result),
            )?,
            KeepOrDrop::DropLo(n) => keep_low(
                v,
                v.len().checked_sub(*n).ok_or_else(|| {
                    format!("Cannot drop {n} dice when there are only {}", v.len())
                })?,
                |result| std::cmp::Reverse(get_number(result)),
            )?,
        };
        Ok(res)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::keep_low;

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
