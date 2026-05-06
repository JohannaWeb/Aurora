use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ElementData {
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub selector: Selector,
    pub declarations: Vec<Declaration>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub parts: Vec<SimpleSelector>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub class_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
}

pub type Specificity = (u8, u8, u8);
