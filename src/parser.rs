use std::sync::LazyLock;

use pest::{iterators::Pair, pratt_parser::PrattParser};
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "dicey.pest"]
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
