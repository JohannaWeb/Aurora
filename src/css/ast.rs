use std::collections::BTreeMap;

pub use super::selectors_impl::AuroraSelectorImpl;

/// A CSS selector parsed by the `selectors` crate.
pub type Selector = selectors::parser::Selector<AuroraSelectorImpl>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ElementData {
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: Selector,
    pub declarations: Vec<Declaration>,
    pub origin: Origin,
    pub source_order: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    UserAgent,
    Author,
}

impl Origin {
    pub fn normal_rank(self) -> u8 {
        match self {
            Self::UserAgent => 0,
            Self::Author => 1,
        }
    }

    pub fn important_rank(self) -> u8 {
        match self {
            Self::Author => 0,
            Self::UserAgent => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
}
