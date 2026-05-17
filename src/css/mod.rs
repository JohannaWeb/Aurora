//! CSS stylesheet parsing, selector matching, and computed style maps.

mod ast;
mod at_rules;
mod display;
mod dom_styles;
mod inline_style;
mod length;
mod properties;
mod selector;
mod shorthand;
mod style_map;
mod style_map_resolve;
mod stylesheet;
mod variables;

#[cfg(test)]
mod tests;

pub use ast::{
    AttrOp, AttrSel, Combinator, Declaration, ElementData, PseudoClass, Rule, Selector,
    SelectorPart, SimpleSelector, Specificity,
};
pub use inline_style::parse_style_text;
#[allow(unused_imports)]
pub use length::{parse_length_value, LengthValue};
pub use properties::{
    AlignItems, BoxSizing, DisplayMode, EdgeSizes, FlexDirection, JustifyContent, Margin,
    MarginValue, TextAlign, WhiteSpace,
};
pub use style_map::StyleMap;
pub use stylesheet::Stylesheet;
