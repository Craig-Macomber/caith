#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, deny(broken_intra_doc_links))]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
//! A dice roller library.

mod dice_expression;
mod expression;

mod command;
mod dice_kind;
mod error;
mod keep_or_drop;
mod parser;

pub use expression::{EvaluatedExpression, Expression, Verbosity};

pub use command::{Command, EvaluatedCommand};

pub use error::*;

use rand::Rng;

/// A source of dice rolls.
pub trait DiceRollSource {
    /// Provides a number to be used by a dice roll.
    fn roll_single_die(&mut self, sides: u64) -> u64;
}

struct RngDiceRollSource<'a, T>
where
    T: Rng,
{
    rng: &'a mut T,
}

impl<T> DiceRollSource for RngDiceRollSource<'_, T>
where
    T: Rng,
{
    fn roll_single_die(&mut self, sides: u64) -> u64 {
        self.rng.random_range(1..1 + sides)
    }
}

/// Something that can be rolled
pub trait Rollable {
    /// Output of the roll
    type Roll;
    /// Evaluate and roll the dices with default Rng source (`rand::thread_rng()`)
    fn roll(&self) -> Self::Roll {
        self.roll_with(&mut rand::rng())
    }

    /// Evaluate and roll the dices with provided rng source
    fn roll_with(&self, rng: &mut impl Rng) -> Self::Roll {
        self.roll_with_source(&mut RngDiceRollSource { rng })
    }

    /// Evaluate and roll the dice with provided dice roll source
    fn roll_with_source(&self, rng: &mut dyn DiceRollSource) -> Self::Roll;
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) struct IteratorDiceRollSource<'a, T>
    where
        T: Iterator<Item = u64>,
    {
        pub iterator: &'a mut T,
    }

    impl<T> DiceRollSource for IteratorDiceRollSource<'_, T>
    where
        T: Iterator<Item = u64>,
    {
        fn roll_single_die(&mut self, sides: u64) -> u64 {
            match self.iterator.next() {
                Some(value) => {
                    if value > sides {
                        panic!("Tried to return {} for a {} sided dice", value, sides)
                    }
                    println!("Dice {}", value);
                    value
                }
                None => panic!("Iterator out of values"),
            }
        }
    }

    #[test]
    fn get_repeat_test() {
        let r = Command::parse("(2d6 + 6) ^ 8 : test").unwrap();
        let roll_mock = vec![3, 5, 3, 5, 3, 5, 3, 5, 3, 5, 3, 5, 3, 5, 3, 5];
        let roll_res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut roll_mock.into_iter(),
            })
            .unwrap();

        assert_eq!(8, roll_res.results().len());
        for res in roll_res.results() {
            assert_eq!(14.0, res.total());
        }
    }

    #[test]
    fn get_repeat_sort_test() {
        let r = Command::parse("(2d6 + 6) ^# 8 : test").unwrap();
        let roll_mock = vec![3, 5, 1, 1, 6, 5, 3, 5, 4, 5, 2, 4, 3, 5, 1, 2];
        let mut expected = roll_mock
            .as_slice()
            .chunks(2)
            .map(|two| two[0] as f64 + two[1] as f64 + 6.0)
            .collect::<Vec<_>>();
        expected.sort_by(f64::total_cmp);
        let roll_res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut roll_mock.into_iter(),
            })
            .unwrap();
        assert_eq!(8, roll_res.results().len());
        let res_vec = roll_res
            .results()
            .iter()
            .map(|r| r.total())
            .collect::<Vec<_>>();
        assert_eq!(expected, res_vec);
    }

    #[test]
    fn get_repeat_sum_test() {
        let r = command::Command::parse("(2d6 + 6) ^+ 2 : test").unwrap();
        let roll_mock = vec![3, 5, 4, 2];
        let expected = roll_mock
            .as_slice()
            .chunks(2)
            .map(|two| two[0] as f64 + two[1] as f64 + 6.0)
            .collect::<Vec<_>>();
        let expected: f64 = expected.iter().sum();
        let roll_res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut roll_mock.into_iter(),
            })
            .unwrap();

        assert_eq!(2, roll_res.results().len());
        assert_eq!(expected, roll_res.total().unwrap());

        assert_eq!(
            roll_res.format(true, Verbosity::Short),
            "(\\[3, 5\\] + 6 = **14**) + (\\[4, 2\\] + 6 = **12**) = **26** : test"
        );
    }

    #[test]
    fn get_single_test() {
        let r = command::Command::parse("2d6 + 6 : test").unwrap();
        let roll_mock = vec![3, 5];
        let expected = roll_mock
            .as_slice()
            .chunks(2)
            .map(|two| two[0] as f64 + two[1] as f64)
            .collect::<Vec<_>>();
        let expected = expected.iter().sum::<f64>() + 6.0;
        let roll_res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut roll_mock.into_iter(),
            })
            .unwrap();
        assert_eq!(roll_res.total().unwrap(), expected);
        assert_eq!(
            roll_res.format(false, Verbosity::Short),
            "[3, 5] + 6 = 14 : test"
        );
    }

    #[test]
    fn one_value_test() {
        let r = command::Command::parse("20").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(20.0, res.total().unwrap());
    }

    #[test]
    fn one_dice_test() {
        let r = command::Command::parse("d20").unwrap();
        let roll_mock = vec![8];
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut roll_mock.into_iter(),
            })
            .unwrap();
        assert_eq!(8.0, res.total().unwrap());
    }

    #[test]
    fn float_mul_test() {
        let r = command::Command::parse("20 * 1.5").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(30.0, res.total().unwrap());
    }

    #[test]
    fn float_signed_mul_test() {
        let r = command::Command::parse("20 * +1.5").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(30.0, res.total().unwrap());
    }

    #[test]
    fn float_neg_signed_mul_test() {
        let r = command::Command::parse("20 * -1.5").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(-30.0, res.total().unwrap());
    }

    #[test]
    fn float_add_test() {
        let r = command::Command::parse("20 + 1.5").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(21.5, res.total().unwrap());
    }

    #[test]
    fn float_signed_add_test() {
        let r = command::Command::parse("20 + +1.5").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(21.5, res.total().unwrap());
    }

    #[test]
    fn float_neg_signed_add_test() {
        let r = command::Command::parse("20 + -1.5").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(18.5, res.total().unwrap());
    }

    #[test]
    fn signed_add_test() {
        let r = command::Command::parse("20 + +5").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(25.0, res.total().unwrap());
    }

    #[test]
    fn signed_neg_add_test() {
        let r = command::Command::parse("20 + -5").unwrap();
        let res = r.roll().unwrap();
        assert_eq!(15.0, res.total().unwrap());
    }

    #[test]
    fn counting_roller_test() {
        let r = command::Command::parse("3d6").unwrap();
        let rolls = vec![3, 6, 3];
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut rolls.into_iter(),
            })
            .unwrap();
        assert_eq!(res.total().unwrap(), 12.0);
    }

    #[test]
    fn target_number_test() {
        let r = command::Command::parse("10d10 t7").unwrap();
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        // We rolled one of every number, with a target number of 7 we should score a success
        // on the 7, 8, 9, and 10. So four total.
        assert_eq!(res.total().unwrap(), 4.0);
    }

    #[test]
    fn target_number_double_test() {
        let r = command::Command::parse("10d10 t7 tt9").unwrap();
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        // We rolled one of every number. That's a success each for the 7 and 8, and two
        // success each for the 9 and 10. So a total of six.
        assert_eq!(res.total().unwrap(), 6.0);
    }

    // Where a user has asked for a doubles threshold that is lower than the single threshold,
    // the single threshold is ignored.
    #[test]
    fn target_number_double_lower_than_target_test() {
        let r = Expression::parse("10d10 tt7 t9").unwrap();
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        // We rolled one of every number. That's two successes each for the 7, 8, 9, and 10.
        // So eight total.
        assert_eq!(res.total(), 8.0);
    }

    // Where a user has asked for a doubles without singles.
    #[test]
    fn target_number_double_only() {
        let r = Expression::parse("10d10 tt8").unwrap();
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        // We rolled one of every number. That's two successes each for the 8, 9, and 10.
        // So six total.
        assert_eq!(res.total(), 6.0);
    }

    #[test]
    fn target_order_of_results() {
        let r = command::Command::parse("2d10").unwrap();
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        let s = format!("{}", res.format(false, Verbosity::Medium));
        assert_eq!(s, "[1, 2] = 3")
    }

    #[test]
    fn keep_highest() {
        let r = command::Command::parse("2d10K1").unwrap();
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        let s = format!("{}", res.format(false, Verbosity::Medium));
        assert_eq!(s, "[Drop(1), 2]K1 = 2");

        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11).rev(),
            })
            .unwrap();
        let s = format!("{}", res.format(false, Verbosity::Medium));
        assert_eq!(s, "[10, Drop(9)]K1 = 10");
    }

    #[test]
    fn keep_lowest() {
        let r = command::Command::parse("2d10k1").unwrap();
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11),
            })
            .unwrap();
        let s = format!("{}", res.format(false, Verbosity::Medium));
        assert_eq!(s, "[1, Drop(2)]k1 = 1");

        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..11).rev(),
            })
            .unwrap();
        let s = format!("{}", res.format(false, Verbosity::Medium));
        assert_eq!(s, "[Drop(10), 9]k1 = 9");
    }

    #[test]
    fn keep_highest_single_1() {
        let r = Expression::parse("2d10K1").unwrap();
        let res = r.roll_with_source(&mut IteratorDiceRollSource {
            iterator: &mut (1..11),
        });
        let s = format!("{}", res.unwrap().format(false, Verbosity::Medium));
        assert_eq!(s, "[Drop(1), 2]K1 = 2");
    }

    #[test]
    fn keep_highest_single_2() {
        let r = Expression::parse("2d10K1").unwrap();
        let res = r.roll_with_source(&mut IteratorDiceRollSource {
            iterator: &mut (1..11).rev(),
        });
        let s = format!("{}", res.unwrap().format(false, Verbosity::Medium));
        assert_eq!(s, "[10, Drop(9)]K1 = 10");
    }

    #[test]
    fn keep_lowest_single() {
        let r = Expression::parse("2d10k1").unwrap();
        let res = r.roll_with_source(&mut IteratorDiceRollSource {
            iterator: &mut (1..11),
        });
        let s = format!("{}", res.unwrap().format(false, Verbosity::Medium));
        assert_eq!(s, "[1, Drop(2)]k1 = 1");

        let res = r.roll_with_source(&mut IteratorDiceRollSource {
            iterator: &mut (1..11).rev(),
        });
        let s = format!("{}", res.unwrap().format(false, Verbosity::Medium));
        assert_eq!(s, "[Drop(10), 9]k1 = 9");
    }

    #[test]
    fn target_enum() {
        let r = command::Command::parse("6d6 t[2,4,6]").unwrap();
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut (1..7),
            })
            .unwrap();

        // We rolled one of every number. That's half of them being even
        assert_eq!(res.total().unwrap(), 3.0);

        let mock = vec![1, 2, 2, 4, 6, 3];
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut mock.into_iter(),
            })
            .unwrap();
        // We rolled one of every number. That's half of them being even
        assert_eq!(res.total().unwrap(), 4.0);

        let mock = vec![1, 3, 3, 4, 6, 3];
        let res = r
            .roll_with_source(&mut IteratorDiceRollSource {
                iterator: &mut mock.into_iter(),
            })
            .unwrap();
        // We rolled one of every number. That's half of them being even
        assert_eq!(res.total().unwrap(), 2.0);
    }

    #[test]
    fn sandbox_test() {
        command::Command::parse("5d6 + 4 * 2")
            .unwrap()
            .roll()
            .unwrap();
    }

    #[test]
    fn minimal() {
        // This should deterministically roll a 1
        let roller = Expression::parse(&"1d1").unwrap();

        let result = roller.roll().unwrap();
        let history = result.format_history(false, Verbosity::Medium);
        let as_string = result.format(false, Verbosity::Medium);

        assert_eq!(result.total(), 1.0);
        assert_eq!(as_string, "[1] = 1");
        assert_eq!(history, "[1]");
    }

    #[test]
    fn reroll() {
        // This should deterministically roll a 1, then reroll 1
        let roller = Expression::parse("1d1 r1").unwrap();

        let result = roller.roll().unwrap();
        let history = result.format_history(false, Verbosity::Medium);
        let as_string = result.format(false, Verbosity::Medium);

        assert_eq!(result.total(), 1.0);
        assert_eq!(as_string, "[1🡲Reroll🡲1]r1 = 1");
        // Rerolls are now displayed in the history
        assert_eq!(history, "[1🡲Reroll🡲1]r1");
    }

    #[test]
    fn no_reroll() {
        // This should deterministically roll a 1, then not reroll anything since 1 > 0
        let roller = Expression::parse("1d1 r0").unwrap();

        let result = roller.roll().unwrap();
        let as_string = result.format(false, Verbosity::Medium);

        assert_eq!(as_string, "[1]r0 = 1");
        assert_eq!(result.total(), 1.0);
    }

    #[test]
    fn infinite_reroll() {
        let roll = Expression::parse("1d1 ir1").unwrap().roll();
        let result = roll.unwrap_err();
        match result {
            RollError::ParseError(_) => assert!(false),
            RollError::ParamError(text) => assert_eq!(
                text,
                "Cannot infinitely reroll dice of 1 or lower since the maximum roll is 1: this would go on forever"
            ),
        }
    }

    #[test]
    fn multiple_reroll() {
        let r = Expression::parse("1d4 ir3").unwrap();
        let res = r.roll_with_source(&mut IteratorDiceRollSource {
            iterator: &mut (1..10),
        });
        let s = format!("{}", res.unwrap().format(false, Verbosity::Medium));
        assert_eq!(s, "[1🡲Reroll🡲2🡲Reroll🡲3🡲Reroll🡲4]ir3 = 4");
    }

    #[test]
    fn d0() {
        let result = Expression::parse("d0").unwrap_err();
        match result {
            RollError::ParseError(e) => {
                assert_eq!(format!("{e}"), "number would be zero for non-zero type")
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn fuzz_regression1() {
        let result = Expression::parse("- 9").unwrap_err();
        match result {
            RollError::ParseError(e) => {
                assert_eq!(format!("{e}"), "invalid digit found in string")
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn fuzz_regression2() {
        let result = Expression::parse("d9d99d9").unwrap().roll().unwrap_err();
        match result {
            RollError::ParamError(e) => {
                assert_eq!(e, "Cannot drop 99 dice when there are only 1")
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn fuzz_regression3() {
        let result = Expression::parse("922222229d979").unwrap_err();
        match result {
            RollError::ParamError(e) => {
                assert_eq!(
                    e,
                    "Exceed maximum allowed number of dice (5000) during parse."
                )
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn fuzz_regression4() {
        let result = Expression::parse("99dFt017").unwrap_err();
        match result {
            RollError::ParseError(e) => {
                assert_eq!(format!("{e}"), "number too large to fit in target type")
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn fuzz_regression5() {
        _ = Expression::parse("99d8255d9!9d5!!3").unwrap().roll();
    }

    #[test]
    fn fuzz_regression6() {
        _ = Expression::parse("4936d999!6").unwrap().roll();
    }

    #[test]
    fn fuzz_regression7() {
        _ = Expression::parse("65d99ie3d99ie3d030303ed939ie3d99ie3D0")
            .unwrap()
            .roll();
    }

    #[test]
    fn fuzz_regression8() {
        let result = command::Command::parse("(9+9)^+70000000").unwrap_err();
        match result {
            RollError::ParamError(e) => {
                assert_eq!(
                    e,
                    "Exceed maximum allowed number of dice (5000) during repeated roll count."
                )
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn fuzz_regression9() {
        let result = command::Command::parse("(d9)^95555555555555555555").unwrap_err();
        match result {
            RollError::ParseError(e) => {
                assert_eq!(e.to_string(), "number too large to fit in target type")
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn fuzz_regression10() {
        _ = Expression::parse("dF!(+)").unwrap();
    }

    #[test]
    fn fuzz_regression11() {
        let e = Expression::parse("dFf1").unwrap();
        let f = format!("{}", e);
        assert_eq!(f, "1dF f(+)");
        let e = Expression::parse(&f).unwrap();
        let f = format!("{}", e);
        assert_eq!(f, "1dF f(+)");
    }

    #[test]
    fn fuzz_regression12() {
        _ = Expression::parse("1dF f(+)").unwrap();
    }

    #[test]
    fn fuzz_regression14() {
        let e = Expression::parse("1dF !( )").unwrap();
        let f = format!("{}", e);
        assert_eq!(f, "1dF !( )");
    }

    #[test]
    fn fuzz_regression15() {
        _ = Expression::parse("d8t(+)").unwrap_err();
    }

    #[test]
    fn round_trip_floats() {
        let data = "9999999999999999943.3";
        let roller = command::Command::parse(data).unwrap();
        let f = format!("{roller}");
        let parsed2 = command::Command::parse(&f).unwrap();
        let f2 = format!("{parsed2}");
        assert_eq!(f, f2);
    }
}
