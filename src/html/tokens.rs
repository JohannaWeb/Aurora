use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    OpenTag(TagToken),
    CloseTag(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TagToken {
    pub(super) tag_name: String,
    pub(super) attributes: BTreeMap<String, String>,
}
