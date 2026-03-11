use crate::ast::item::Item;
use super::{Parser, ParseError};

pub fn parse_item(_parser: &mut Parser<'_>) -> Result<Item, ParseError> {
    Err(_parser.error("item parsing not yet implemented"))
}
