use std::collections::BTreeMap;

pub use super::selectors_impl::AuroraSelectorImpl;

/// A CSS selector parsed by the `selectors` crate.
pub type Selector = selectors::parser::Selector<AuroraSelectorImpl>;

/// Specificity as a u32 (packed 0xAA_BB_CC: A=id, B=class, C=type).
pub type Specificity = u32;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ElementData {
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: Selector,
    pub declarations: Vec<Declaration>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
}
