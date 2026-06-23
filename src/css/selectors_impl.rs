//! Integration with the `selectors` crate.
//!
//! Provides:
//! - `AuroraSelectorImpl` — the `SelectorImpl` glue type.
//! - `CascadeElement` — wraps `ElementData` + ancestor/sibling context so the
//!   `selectors` crate can traverse the tree for combinators and pseudo-classes.
//! - `AurSelectorParser` — the `selectors::parser::Parser` used when calling
//!   `SelectorList::parse` from inside `cssparser::QualifiedRuleParser::parse_prelude`.

use std::borrow::Borrow;
use std::fmt;

use cssparser::ToCss;
use selectors::attr::{
    AttrSelectorOperation, AttrSelectorOperator, CaseSensitivity, NamespaceConstraint,
};
use selectors::bloom::BloomFilter;
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::{ElementSelectorFlags, matches_selector};

/// Error type for selector parsing — wraps `SelectorParseErrorKind` as opaque.
#[derive(Debug)]
pub struct AurSelectorParseError;

impl<'i> From<selectors::parser::SelectorParseErrorKind<'i>> for AurSelectorParseError {
    fn from(_: selectors::parser::SelectorParseErrorKind<'i>) -> Self {
        AurSelectorParseError
    }
}
use selectors::OpaqueElement;
use selectors::parser::{
    NonTSPseudoClass, ParseRelative, PseudoElement, SelectorImpl, SelectorList,
};

use super::ElementData;

// ─── CssString ───────────────────────────────────────────────────────────────

/// Newtype over `String` satisfying all bounds required by `SelectorImpl` associated types.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct CssString(pub String);

impl From<&str> for CssString {
    fn from(s: &str) -> Self {
        CssString(s.to_string())
    }
}

impl Borrow<str> for CssString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl ToCss for CssString {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_str(&self.0)
    }
}

impl precomputed_hash::PrecomputedHash for CssString {
    fn precomputed_hash(&self) -> u32 {
        fnv1a(&self.0)
    }
}

fn fnv1a(s: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

// ─── NonTSPseudoClass ─────────────────────────────────────────────────────────

/// Non-tree-structural pseudo-classes (state pseudo-classes, :lang, etc.)
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AurNonTSPseudoClass {
    Link,
    Visited,
    Hover,
    Focus,
    FocusWithin,
    Active,
    Checked,
    Disabled,
    Enabled,
    Placeholder,
    Lang(CssString),
    /// `:host` — matches custom elements (tag names containing `-`).
    Host,
    /// `:host(selector)` — matches custom elements that also match the inner selector.
    HostWith(Box<SelectorList<AuroraSelectorImpl>>),
    /// Catch-all for unrecognised pseudo-classes — always returns false.
    Unknown,
}

impl std::hash::Hash for AurNonTSPseudoClass {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        if let Self::Lang(s) = self {
            s.hash(state);
        }
        // HostWith: SelectorList has no Hash impl; hash nothing extra.
    }
}

impl ToCss for AurNonTSPseudoClass {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        match self {
            Self::Link => dest.write_str(":link"),
            Self::Visited => dest.write_str(":visited"),
            Self::Hover => dest.write_str(":hover"),
            Self::Focus => dest.write_str(":focus"),
            Self::FocusWithin => dest.write_str(":focus-within"),
            Self::Active => dest.write_str(":active"),
            Self::Checked => dest.write_str(":checked"),
            Self::Disabled => dest.write_str(":disabled"),
            Self::Enabled => dest.write_str(":enabled"),
            Self::Placeholder => dest.write_str("::placeholder"),
            Self::Lang(l) => write!(dest, ":lang({})", l.0),
            Self::Host => dest.write_str(":host"),
            Self::HostWith(_) => dest.write_str(":host(...)"),
            Self::Unknown => dest.write_str(":unknown"),
        }
    }
}

impl NonTSPseudoClass for AurNonTSPseudoClass {
    type Impl = AuroraSelectorImpl;
    fn is_active_or_hover(&self) -> bool {
        matches!(self, Self::Active | Self::Hover)
    }
    fn is_user_action_state(&self) -> bool {
        matches!(
            self,
            Self::Active | Self::Hover | Self::Focus | Self::FocusWithin
        )
    }
}

// ─── PseudoElement ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AurPseudoElement {
    Before,
    After,
    Placeholder,
    Selection,
}

impl ToCss for AurPseudoElement {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        match self {
            Self::Before => dest.write_str("::before"),
            Self::After => dest.write_str("::after"),
            Self::Placeholder => dest.write_str("::placeholder"),
            Self::Selection => dest.write_str("::selection"),
        }
    }
}

impl PseudoElement for AurPseudoElement {
    type Impl = AuroraSelectorImpl;
}

// ─── SelectorImpl ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuroraSelectorImpl;

impl SelectorImpl for AuroraSelectorImpl {
    type ExtraMatchingData<'a> = std::marker::PhantomData<&'a ()>;
    type AttrValue = CssString;
    type Identifier = CssString;
    type LocalName = CssString;
    type NamespaceUrl = CssString;
    type NamespacePrefix = CssString;
    type BorrowedLocalName = str;
    type BorrowedNamespaceUrl = str;
    type NonTSPseudoClass = AurNonTSPseudoClass;
    type PseudoElement = AurPseudoElement;
}

// ─── SelectorParser ───────────────────────────────────────────────────────────

/// Implements `selectors::parser::Parser` so we can call `SelectorList::parse`.
pub struct AurSelectorParser;

impl<'i> selectors::parser::Parser<'i> for AurSelectorParser {
    type Impl = AuroraSelectorImpl;
    type Error = AurSelectorParseError;

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_has(&self) -> bool {
        false
    }

    fn parse_slotted(&self) -> bool {
        true
    }

    fn parse_non_ts_pseudo_class(
        &self,
        _location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<AurNonTSPseudoClass, cssparser::ParseError<'i, Self::Error>> {
        let pc = match name.as_ref() {
            "link" => AurNonTSPseudoClass::Link,
            "visited" => AurNonTSPseudoClass::Visited,
            "hover" => AurNonTSPseudoClass::Hover,
            "focus" => AurNonTSPseudoClass::Focus,
            "focus-within" => AurNonTSPseudoClass::FocusWithin,
            "active" => AurNonTSPseudoClass::Active,
            "checked" => AurNonTSPseudoClass::Checked,
            "disabled" => AurNonTSPseudoClass::Disabled,
            "enabled" => AurNonTSPseudoClass::Enabled,
            "placeholder" => AurNonTSPseudoClass::Placeholder,
            "host" => AurNonTSPseudoClass::Host,
            _ => AurNonTSPseudoClass::Unknown,
        };
        Ok(pc)
    }

    fn parse_non_ts_functional_pseudo_class<'t>(
        &self,
        name: cssparser::CowRcStr<'i>,
        parser: &mut cssparser::Parser<'i, 't>,
        _after_part: bool,
    ) -> Result<AurNonTSPseudoClass, cssparser::ParseError<'i, Self::Error>> {
        match name.as_ref() {
            "lang" => {
                let lang = parser.expect_ident_or_string()?.to_owned();
                Ok(AurNonTSPseudoClass::Lang(CssString(lang.to_string())))
            }
            "host" => match SelectorList::parse(&AurSelectorParser, parser, ParseRelative::No) {
                Ok(list) => Ok(AurNonTSPseudoClass::HostWith(Box::new(list))),
                Err(_) => {
                    while parser.next().is_ok() {}
                    Ok(AurNonTSPseudoClass::Host)
                }
            },
            _ => {
                // Drain the argument list so the parser stays in sync.
                while parser.next().is_ok() {}
                Ok(AurNonTSPseudoClass::Unknown)
            }
        }
    }

    fn parse_pseudo_element(
        &self,
        location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<AurPseudoElement, cssparser::ParseError<'i, Self::Error>> {
        let pe = match name.as_ref() {
            "before" => AurPseudoElement::Before,
            "after" => AurPseudoElement::After,
            "placeholder" => AurPseudoElement::Placeholder,
            "selection" => AurPseudoElement::Selection,
            _ => return Err(location.new_custom_error(AurSelectorParseError)),
        };
        Ok(pe)
    }
}

/// Parse a comma-separated selector string into a `SelectorList`.
/// Returns `None` if parsing fails entirely.
pub fn parse_selector_list(selector_str: &str) -> Option<SelectorList<AuroraSelectorImpl>> {
    let mut input = cssparser::ParserInput::new(selector_str);
    let mut parser = cssparser::Parser::new(&mut input);
    SelectorList::parse(&AurSelectorParser, &mut parser, ParseRelative::No).ok()
}

// ─── CascadeElement ───────────────────────────────────────────────────────────

/// Wraps `ElementData` + tree context so `selectors::Element` can be implemented.
///
/// The `ancestors` slice holds outermost→immediate-parent `ElementData`.
/// The `siblings` slice holds all element siblings at this level.
/// `sibling_index` is the 0-based position of this element among `siblings`.
#[derive(Clone, Debug)]
pub struct CascadeElement<'a> {
    pub element: &'a ElementData,
    pub ancestors: &'a [ElementData],
    pub siblings: &'a [ElementData],
    pub sibling_index: usize,
}

impl<'a> CascadeElement<'a> {
    #[allow(dead_code)]
    pub fn new(
        element: &'a ElementData,
        ancestors: &'a [ElementData],
        siblings: &'a [ElementData],
        sibling_index: usize,
    ) -> Self {
        Self {
            element,
            ancestors,
            siblings,
            sibling_index,
        }
    }
}

impl<'a> selectors::Element for CascadeElement<'a> {
    type Impl = AuroraSelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(self.element)
    }

    fn parent_element(&self) -> Option<Self> {
        if self.ancestors.is_empty() {
            return None;
        }
        let i = self.ancestors.len() - 1;
        Some(CascadeElement {
            element: &self.ancestors[i],
            ancestors: &self.ancestors[..i],
            siblings: &[], // parent's siblings not tracked
            sibling_index: 0,
        })
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        if self.sibling_index == 0 {
            return None;
        }
        let i = self.sibling_index - 1;
        Some(CascadeElement {
            element: &self.siblings[i],
            ancestors: self.ancestors,
            siblings: self.siblings,
            sibling_index: i,
        })
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let next = self.sibling_index + 1;
        if next >= self.siblings.len() {
            return None;
        }
        Some(CascadeElement {
            element: &self.siblings[next],
            ancestors: self.ancestors,
            siblings: self.siblings,
            sibling_index: next,
        })
    }

    fn first_element_child(&self) -> Option<Self> {
        None // children not available in cascade context
    }

    fn is_html_element_in_html_document(&self) -> bool {
        true
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.element.tag_name.eq_ignore_ascii_case(local_name)
    }

    fn has_namespace(&self, ns: &str) -> bool {
        ns.is_empty() // HTML elements are in the empty/HTML namespace
    }

    fn is_same_type(&self, other: &Self) -> bool {
        self.element
            .tag_name
            .eq_ignore_ascii_case(&other.element.tag_name)
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&CssString>,
        local_name: &CssString,
        operation: &AttrSelectorOperation<&CssString>,
    ) -> bool {
        // Ignore namespace for HTML.
        if let NamespaceConstraint::Specific(ns) = ns {
            if !ns.0.is_empty() {
                return false;
            }
        }
        let name = local_name.0.to_ascii_lowercase();
        let actual = self.element.attributes.get(&name).map(String::as_str);

        match operation {
            AttrSelectorOperation::Exists => actual.is_some(),
            AttrSelectorOperation::WithValue {
                operator,
                case_sensitivity,
                value,
            } => {
                let Some(actual) = actual else { return false };
                let (a, e) = match case_sensitivity {
                    CaseSensitivity::AsciiCaseInsensitive => {
                        (actual.to_ascii_lowercase(), value.0.to_ascii_lowercase())
                    }
                    CaseSensitivity::CaseSensitive => (actual.to_string(), value.0.clone()),
                };
                match operator {
                    AttrSelectorOperator::Equal => a == e,
                    AttrSelectorOperator::Includes => a.split_whitespace().any(|w| w == e),
                    AttrSelectorOperator::DashMatch => a == e || a.starts_with(&format!("{e}-")),
                    AttrSelectorOperator::Prefix => a.starts_with(e.as_str()),
                    AttrSelectorOperator::Suffix => a.ends_with(e.as_str()),
                    AttrSelectorOperator::Substring => a.contains(e.as_str()),
                }
            }
        }
    }

    fn match_non_ts_pseudo_class(
        &self,
        pc: &AurNonTSPseudoClass,
        _context: &mut MatchingContext<AuroraSelectorImpl>,
    ) -> bool {
        match pc {
            AurNonTSPseudoClass::Link => {
                self.element.tag_name.eq_ignore_ascii_case("a")
                    && self.element.attributes.contains_key("href")
            }
            AurNonTSPseudoClass::Lang(lang) => {
                let target = lang.0.to_ascii_lowercase();
                // Check element's own lang attr, then walk ancestors.
                if let Some(el_lang) = self.element.attributes.get("lang") {
                    let el_lang = el_lang.to_ascii_lowercase();
                    return el_lang == target || el_lang.starts_with(&format!("{target}-"));
                }
                for anc in self.ancestors.iter().rev() {
                    if let Some(al) = anc.attributes.get("lang") {
                        let al = al.to_ascii_lowercase();
                        return al == target || al.starts_with(&format!("{target}-"));
                    }
                }
                false
            }
            AurNonTSPseudoClass::Host => self.element.tag_name.contains('-'),
            AurNonTSPseudoClass::HostWith(selector_list) => {
                if !self.element.tag_name.contains('-') {
                    return false;
                }
                selector_list.slice().iter().any(|sel| {
                    element_matches(
                        sel,
                        self.element,
                        self.ancestors,
                        self.siblings,
                        self.sibling_index,
                    )
                })
            }
            // All other state pseudo-classes need runtime interaction tracking.
            _ => false,
        }
    }

    fn match_pseudo_element(
        &self,
        _pe: &AurPseudoElement,
        _context: &mut MatchingContext<AuroraSelectorImpl>,
    ) -> bool {
        false
    }

    fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {}

    fn add_element_unique_hashes(&self, filter: &mut BloomFilter) -> bool {
        filter.insert_hash(fnv1a(&self.element.tag_name));
        if let Some(id) = self.element.attributes.get("id") {
            filter.insert_hash(fnv1a(id));
        }
        true
    }

    fn is_link(&self) -> bool {
        self.element.tag_name.eq_ignore_ascii_case("a")
            && self.element.attributes.contains_key("href")
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(&self, id: &CssString, case_sensitivity: CaseSensitivity) -> bool {
        match self.element.attributes.get("id") {
            None => false,
            Some(val) => match case_sensitivity {
                CaseSensitivity::CaseSensitive => val == &id.0,
                CaseSensitivity::AsciiCaseInsensitive => val.eq_ignore_ascii_case(&id.0),
            },
        }
    }

    fn has_class(&self, name: &CssString, case_sensitivity: CaseSensitivity) -> bool {
        let class_attr = self
            .element
            .attributes
            .get("class")
            .map(String::as_str)
            .unwrap_or("");
        class_attr
            .split_whitespace()
            .any(|cls| match case_sensitivity {
                CaseSensitivity::CaseSensitive => cls == name.0,
                CaseSensitivity::AsciiCaseInsensitive => cls.eq_ignore_ascii_case(&name.0),
            })
    }

    fn has_custom_state(&self, _name: &CssString) -> bool {
        false
    }

    fn imported_part(&self, _name: &CssString) -> Option<CssString> {
        None
    }

    fn is_part(&self, _name: &CssString) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        false // DOM children not available here
    }

    fn is_root(&self) -> bool {
        self.ancestors.is_empty()
    }
}

// ─── Public matching helper ───────────────────────────────────────────────────

/// Match a single `selectors::Selector` against a `CascadeElement`.
pub fn element_matches(
    selector: &selectors::parser::Selector<AuroraSelectorImpl>,
    element: &ElementData,
    ancestors: &[ElementData],
    siblings: &[ElementData],
    sibling_index: usize,
) -> bool {
    let el = CascadeElement {
        element,
        ancestors,
        siblings,
        sibling_index,
    };
    let mut caches = SelectorCaches::default();
    let mut ctx = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    matches_selector(selector, 0, None, &el, &mut ctx)
}
