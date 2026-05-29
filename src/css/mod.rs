//! CSS stylesheet parsing, selector matching, and computed style maps.

mod ast;
pub(crate) mod calc;
mod display;
mod dom_styles;
mod inline_style;
mod length;
mod properties;
pub mod selectors_impl;
mod shorthand;
mod style_map;
mod style_map_resolve;
mod stylesheet;

#[cfg(test)]
mod tests;

pub use ast::{Declaration, ElementData, Origin, Rule, Selector};
pub use inline_style::parse_style_text;
#[allow(unused_imports)]
pub use length::{LengthValue, parse_length_value};
pub use properties::{
    AlignItems, BoxSizing, DisplayMode, EdgeSizes, FlexDirection, JustifyContent, Margin,
    MarginValue, TextAlign, WhiteSpace,
};
pub use selectors_impl::CascadeElement;
pub use style_map::StyleMap;
pub use stylesheet::Stylesheet;
