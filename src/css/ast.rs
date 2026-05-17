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

/// How a selector part connects to the part that precedes it in the selector.
/// Stored on each `SelectorPart`; ignored for the first part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// Whitespace — any ancestor.
    Descendant,
    /// `>` — direct parent only.
    Child,
    /// `+` — immediately preceding element sibling.
    Adjacent,
    /// `~` — any preceding element sibling.
    Sibling,
}

/// One compound selector together with the combinator that precedes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorPart {
    /// How this part connects to the part before it.  Ignored for `parts[0]`.
    pub combinator: Combinator,
    pub simple: SimpleSelector,
}

/// A full CSS selector (compound selectors joined by combinators).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub parts: Vec<SelectorPart>,
}

/// Attribute selector operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrOp {
    /// `[attr]` — attribute must be present.
    Exists,
    /// `[attr=val]` — exact equality.
    Equals,
    /// `[attr~=val]` — word in whitespace-separated list.
    Includes,
    /// `[attr|=val]` — equals `val` or starts with `val-`.
    DashPrefix,
    /// `[attr^=val]` — starts with `val`.
    Prefix,
    /// `[attr$=val]` — ends with `val`.
    Suffix,
    /// `[attr*=val]` — contains `val`.
    Substring,
}

/// A single attribute selector, e.g. `[type=checkbox]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrSel {
    pub name: String,
    pub op: AttrOp,
    pub value: Option<String>,
}

/// Pseudo-classes Aurora parses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoClass {
    /// `:not(<simple>)` — CSS3 single-simple-selector form.
    Not(Box<SimpleSelector>),
    /// `:root`
    Root,
    /// `:first-child` — requires sibling context; returns false in cascade path.
    FirstChild,
    /// `:last-child`
    LastChild,
    /// `:nth-child(an+b)` stored as `(a, b)`.
    NthChild(i32, i32),
    /// `:first-of-type`
    FirstOfType,
    /// `:last-of-type`
    LastOfType,
    /// State pseudo-classes Aurora cannot match statically.
    Hover,
    Focus,
    Active,
    Checked,
    Disabled,
    Enabled,
    Visited,
    /// Any unrecognised pseudo-class — never matches so it doesn't silently apply rules.
    Unknown(String),
}

/// A simple (compound) selector: tag, id, classes, attributes, pseudo-classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub class_names: Vec<String>,
    pub attributes: Vec<AttrSel>,
    pub pseudo_classes: Vec<PseudoClass>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
}

pub type Specificity = (u8, u8, u8);
