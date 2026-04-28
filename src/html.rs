// Import DOM node types
// RUST FUNDAMENTAL: Importing `Node` here lets the parser build DOM values without spelling the full module path every time.
use crate::dom::Node;
// Import BTreeMap for storing attributes in sorted order
use std::collections::BTreeMap;

// Enum representing different token types found in HTML
// RUST FUNDAMENTAL: A Rust enum is one type that can represent several different shapes of data.
// Each value stores both the active variant and that variant's payload, so `Token` can be a tag token or a text token
// without needing inheritance or separate parallel types.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    // Opening tag with tag name and attributes
    // RUST FUNDAMENTAL: This tuple variant stores one `TagToken` value inside the enum.
    // Pattern matching lets us extract that payload with syntax like `Token::OpenTag(tag)`.
    OpenTag(TagToken),

    // Closing tag with tag name
    // RUST FUNDAMENTAL: This variant stores an owned `String`, so the token fully owns the closing tag name.
    // That avoids borrowing from the original HTML source and keeps the token stream self-contained.
    CloseTag(String),

    // Text content between tags
    // RUST FUNDAMENTAL: Because this is an owned `String`, the token owns its text bytes outright.
    // That makes the token stream simpler to keep around, because it does not borrow pieces of the original input.
    Text(String),
}

// Struct holding data about an opening tag
// RUST FUNDAMENTAL: Small private structs like this are often used as parser-internal helper representations.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TagToken {
    // Name of the tag (e.g., "div", "p")
    tag_name: String,

    // Map of attribute names to their values
    // RUST FUNDAMENTAL: `BTreeMap` stores entries ordered by key, so iterating attributes is deterministic.
    // That can make printing, debugging, and test output much easier to reason about than a hash-based map.
    attributes: BTreeMap<String, String>,
}

// Parser struct for converting HTML strings to DOM
// RUST FUNDAMENTAL: The lifetime parameter `'a` says this parser borrows some input string for as long as the parser exists.
// `Parser<'a>` therefore means "a parser that is tied to data guaranteed to stay alive for lifetime `'a`".
pub struct Parser<'a> {
    // Pre-tokenized HTML as vector of tokens
    // RUST FUNDAMENTAL: `Vec<Token>` is an owning, growable collection stored on the heap.
    // The parser owns these tokens outright, which means parsing works over pre-built data instead of borrowing from a tokenizer on the fly.
    tokens: Vec<Token>,

    // Current position in token stream
    // RUST FUNDAMENTAL: `usize` is Rust's pointer-sized unsigned integer type.
    // It is the standard choice for indexing slices and vectors because it matches the platform's address size.
    position: usize,

    // Reference to original HTML source (kept but not used)
    // RUST FUNDAMENTAL: `&'a str` is a borrowed string slice.
    // It does not own text data itself; it is only a view into bytes owned somewhere else.
    // The `'a` lifetime ties that borrow to the parser so Rust can prove the parser never outlives its source.
    // #[allow(dead_code)] suppresses warning for intentionally unused field (kept for potential future use)
    #[allow(dead_code)]
    source: &'a str,
}

// Implementation of Parser methods
// RUST FUNDAMENTAL: `impl<'a> Parser<'a>` means "define methods for `Parser` for any valid lifetime `'a`".
// Lifetimes are a kind of generic parameter, but instead of describing data shape they describe how long borrows are valid.
impl<'a> Parser<'a> {
    // Create a new parser from HTML source string
    // RUST FUNDAMENTAL: The constructor accepts a borrowed string and stores that borrow in the parser.
    // Because both use the same `'a`, the compiler guarantees the parser cannot outlive the input string.
    pub fn new(source: &'a str) -> Self {
        // Tokenize HTML string and create parser state
        Self {
            // Pre-tokenize HTML into token stream
            // RUST FUNDAMENTAL: This is a good example of an API boundary that turns borrowed input into owned output.
            // `tokenize` reads from `&str`, then returns a `Vec<Token>` that no longer depends on the caller's borrow.
            tokens: tokenize(source),

            // Start at beginning of token stream
            position: 0,

            // Keep reference to original source
            // RUST FUNDAMENTAL: Storing `source` here is what actually makes the struct carry the borrow.
            // Because of that field, the borrow checker treats the entire parser as being tied to the source lifetime.
            source,
        }
    }

    // Parse entire HTML document into DOM tree
    // RUST FUNDAMENTAL: `&mut self` means this method needs exclusive access to the parser while it runs.
    // That is required because parsing advances `self.position`, which mutates parser state.
    pub fn parse_document(&mut self) -> crate::dom::NodePtr {
        // Initialize vector to collect top-level nodes
        // RUST FUNDAMENTAL: `Vec::new()` creates an empty growable collection with no elements yet.
        // As nodes are pushed, the vector may allocate more capacity behind the scenes.
        let mut children = Vec::new();

        // Parse nodes until end of token stream
        // RUST FUNDAMENTAL: A `while` loop is a good fit when the number of iterations depends on mutable state.
        // Here the loop continues until parsing reaches the end of the token stream.
        while !self.is_eof() {
            // Try to parse a node at current position
            // RUST FUNDAMENTAL: `if let` is a concise way to say "run this branch only when the value matches a pattern".
            // It is especially common with `Option` and `Result` when you care about one success case and want a simple fallback.
            if let Some(node) = self.parse_node() {
                // Add parsed node to children vector
                // RUST FUNDAMENTAL: `Vec::push` appends to the end and takes ownership of the value you pass in.
                // After this call, the vector is responsible for storing that node pointer.
                children.push(node);
            } else {
                // If no node found, advance position
                // RUST FUNDAMENTAL: Because the method has `&mut self`, it can safely change internal fields like `position`.
                self.position += 1;
            }
        }

        // Wrap children in document node
        // RUST FUNDAMENTAL: Returning the document node here moves the entire `children` vector into `Node::document`.
        // No deep copy is needed; ownership is simply transferred to the new node.
        Node::document(children)
    }

    // Parse a single node (text, element, or closing tag)
    // RUST FUNDAMENTAL: `Option<T>` is Rust's standard way to represent "a value might be absent".
    // Returning `Option<NodePtr>` makes it explicit that parsing at the current position may or may not yield a node.
    fn parse_node(&mut self) -> Option<crate::dom::NodePtr> {
        // Get current token without advancing
        // RUST FUNDAMENTAL: The `?` operator works on `Option` as well as `Result`.
        // In an `Option`-returning function, `self.peek()?` means "get the token, or return `None` immediately if there is no token".
        match self.peek()? {
            // If current token is text
            // RUST FUNDAMENTAL: `match` is Rust's primary branching construct for enums.
            // The compiler checks that every possible variant is handled, which prevents missing cases by accident.
            Token::Text(text) => {
                // Clone text content
                // RUST FUNDAMENTAL: `peek()` gives us a borrowed view of the token, not ownership of it.
                // Because we cannot move fields out of borrowed data, cloning gives us a new owned `String` to work with.
                let text = text.clone();

                // Advance to next token
                self.position += 1;

                // Return text node
                // RUST FUNDAMENTAL: `Some(...)` is the "present value" variant of `Option`.
                // Wrapping the node in `Some` makes the success case explicit in the type system.
                Some(Node::text(text))
            }

            // If current token is opening tag
            Token::OpenTag(tag) => {
                // Clone tag data
                let tag = tag.clone();

                // Advance to next token
                self.position += 1;

                // Check if tag is void (self-closing)
                // RUST FUNDAMENTAL: An `if` expression in Rust has a value, so both branches must produce compatible types.
                // In this case both branches evaluate to `Option<NodePtr>`.
                if is_void_tag(&tag.tag_name) {
                    // Create element without children for void tags
                    // RUST FUNDAMENTAL: Void elements still use the same element constructor,
                    // but they pass an empty `Vec` because HTML rules say they cannot have child nodes.
                    Some(Node::element_with_attributes(
                        tag.tag_name,
                        tag.attributes,
                        Vec::new(),
                    ))
                } else {
                    // Parse element with children for normal tags
                    // RUST FUNDAMENTAL: This is recursive descent parsing.
                    // Each nested element causes another function call, which mirrors the nested structure of the HTML tree.
                    Some(self.parse_element(tag))
                }
            }

            // If current token is closing tag, skip it
            // RUST FUNDAMENTAL: `_` is the wildcard pattern, meaning "match anything and ignore the value".
            // Returning `None` here tells the caller that a closing tag does not itself produce a standalone DOM node.
            Token::CloseTag(_) => None,
        }
    }

    // Parse an element and its children until matching close tag
    fn parse_element(&mut self, tag: TagToken) -> crate::dom::NodePtr {
        // Initialize vector to collect child nodes
        // RUST FUNDAMENTAL: This vector will accumulate owned child pointers before the final element node is constructed.
        let mut children = Vec::new();

        // Parse children until matching close tag or end
        // RUST FUNDAMENTAL: `while let Some(token) = ...` is a nice parser pattern for "keep going while input remains".
        while let Some(token) = self.peek() {
            // Match token type
            match token {
                // If matching close tag found, stop parsing children
                // RUST FUNDAMENTAL: Match guards can compare borrowed values without moving them out of the token.
                Token::CloseTag(close_tag) if close_tag == &tag.tag_name => {
                    // Advance past close tag
                    self.position += 1;
                    // Exit loop
                    // RUST FUNDAMENTAL: `break` exits only the nearest loop, not the whole function.
                    break;
                }
                // If non-matching close tag, stop parsing (mismatched tags)
                // RUST FUNDAMENTAL: This parser chooses to stop at an unexpected close tag rather than throwing an error.
                // That kind of "best effort" recovery is common in HTML parsing.
                Token::CloseTag(_) => break,
                // For other tokens, try to parse as child node
                _ => {
                    // Attempt to parse child node
                    if let Some(node) = self.parse_node() {
                        // Add child node to children vector
                        children.push(node);
                    }
                }
            }
        }

        // Create element with collected children
        // RUST FUNDAMENTAL: Constructing the final node at the end means the parser can gather all children first
        // and then move them into the resulting element in one step.
        Node::element_with_attributes(tag.tag_name, tag.attributes, children)
    }

    // Peek at current token without consuming it
    fn peek(&self) -> Option<&Token> {
        // Return reference to token at current position
        // RUST FUNDAMENTAL: `Vec::get` returns `Option<&T>` instead of panicking, which makes out-of-bounds access explicit and safe.
        self.tokens.get(self.position)
    }

    // Check if we've reached end of token stream
    fn is_eof(&self) -> bool {
        // True if position is at or past token count
        // RUST FUNDAMENTAL: Comparing indexes against `.len()` is the standard way to implement end-of-input checks for slices and vectors.
        self.position >= self.tokens.len()
    }
}

// Convert HTML source string into token stream
fn tokenize(source: &str) -> Vec<Token> {
    // Initialize result token vector
    // RUST FUNDAMENTAL: Tokenization is often modeled as "read input, emit a sequence of simpler units" before parsing starts.
    let mut tokens = Vec::new();
    // Initialize text accumulation buffer
    // RUST FUNDAMENTAL: Mutable `String` buffers are useful when building text incrementally one character or chunk at a time.
    let mut text_buffer = String::new();
    // Current byte offset in source
    // RUST FUNDAMENTAL: String indexing in Rust is byte-based, not character-based, because UTF-8 characters have variable width.
    let mut index = 0;

    // Process source string character by character
    while index < source.len() {
        // Get substring from current position to end
        // RUST FUNDAMENTAL: Slicing a `str` like this produces another borrowed `&str` view into the same underlying bytes.
        let rest = &source[index..];
        // Get next character, break if none
        // RUST FUNDAMENTAL: `chars().next()` returns an `Option<char>` because the slice may be empty.
        // `let Some(ch) = ... else { break; }` is a concise early-exit pattern.
        let Some(ch) = rest.chars().next() else {
            break;
        };

        // Check if character is tag start
        // RUST FUNDAMENTAL: Comparing a `char` to a character literal like `'<'` is a direct scalar comparison, not a string comparison.
        if ch == '<' {
            // If text buffer has content, emit it as text token
            if !text_buffer.is_empty() {
                // Collapse consecutive whitespace
                // RUST FUNDAMENTAL: Borrowing `&text_buffer` avoids cloning the accumulated text just to normalize it.
                let collapsed = collapse_whitespace(&text_buffer);
                // Only emit if non-whitespace content
                // RUST FUNDAMENTAL: `.trim()` returns a borrowed slice with leading and trailing whitespace removed.
                if !collapsed.trim().is_empty() {
                    // Decode HTML entities and create text token
                    tokens.push(Token::Text(decode_entities(&collapsed)));
                }
            }
            // Clear text buffer for next segment
            // RUST FUNDAMENTAL: `.clear()` empties the existing `String` without dropping the buffer allocation, so it can be reused.
            text_buffer.clear();

            // Find end of tag (next '>'), skip if not found
            // RUST FUNDAMENTAL: `let Some(x) = ... else { ... }` is useful when the failure case should immediately handle recovery.
            let Some(tag_end_offset) = find_tag_end(rest) else {
                // Not a tag, treat '<' as text
                // RUST FUNDAMENTAL: `String::push` appends a single `char`, which is different from `push_str` for `&str`.
                text_buffer.push(ch);
                // Advance by character width (UTF-8 safe)
                // RUST FUNDAMENTAL: `char::len_utf8()` returns how many bytes this Unicode scalar occupies in UTF-8.
                index += ch.len_utf8();
                continue;
            };

            // Skip DOCTYPE declarations (not needed for DOM)
            // RUST FUNDAMENTAL: `starts_with` checks a borrowed string prefix without allocating a new string.
            if rest.starts_with("<!DOCTYPE") || rest.starts_with("<!doctype") {
                // Skip entire DOCTYPE tag
                index += tag_end_offset + 1;
                continue;
            }

            // Skip HTML comments
            if rest.starts_with("<!--") {
                // Find comment end marker
                // RUST FUNDAMENTAL: `str::find` returns an `Option<usize>` byte offset when a substring is found.
                if let Some(comment_end) = rest.find("-->") {
                    // Skip past comment end
                    index += comment_end + 3;
                } else {
                    // Unterminated comment, skip to end
                    // RUST FUNDAMENTAL: This parser takes a recovery approach here rather than erroring out.
                    index += 4;
                }
                continue;
            }

            // Extract tag content between < and >
            // RUST FUNDAMENTAL: String slicing must happen on valid UTF-8 boundaries.
            // Here the bounds come from previously scanned positions, so the slice is safe.
            let tag = rest[1..tag_end_offset].trim();
            // Advance past closing >
            index += tag_end_offset + 1;

            // Check if tag is closing tag (starts with /)
            // RUST FUNDAMENTAL: `strip_prefix` returns `Some(remainder)` if the prefix exists, otherwise `None`.
            if let Some(stripped) = tag.strip_prefix('/') {
                // Create close tag token with tag name
                // RUST FUNDAMENTAL: `.trim()` borrows a cleaned-up view; `.to_string()` then allocates owned storage for the token.
                tokens.push(Token::CloseTag(stripped.trim().to_string()));
            } else if !tag.is_empty() {
                // Parse opening tag into tag name and attributes
                let open_tag = parse_open_tag(tag);
                // Check if tag needs raw text parsing (script, style)
                let raw_text_tag = is_raw_text_tag(&open_tag.tag_name);
                // Keep tag name for later use
                let tag_name = open_tag.tag_name.clone();
                // Emit opening tag token
                tokens.push(Token::OpenTag(open_tag));

                // Special handling for raw text tags
                if raw_text_tag {
                    // Build closing tag string to search for
                    // RUST FUNDAMENTAL: `format!` can interpolate local variables directly by name into a new `String`.
                    let close_tag = format!("</{tag_name}>");
                    // Find position of closing tag
                    if let Some(close_offset) = source[index..].find(&close_tag) {
                        // Extract raw text between tags
                        // RUST FUNDAMENTAL: Borrowed slicing plus a decode step lets the parser avoid copying more than needed.
                        let raw_text = decode_entities(&source[index..index + close_offset]);
                        // Emit non-empty text tokens
                        if !raw_text.trim().is_empty() {
                            tokens.push(Token::Text(raw_text.trim().to_string()));
                        }
                        // Emit closing tag token
                        tokens.push(Token::CloseTag(tag_name));
                        // Skip past raw text and closing tag
                        index += close_offset + close_tag.len();
                    } else {
                        // No closing tag found, emit remaining as text
                        let raw_text = decode_entities(&source[index..]);
                        if !raw_text.trim().is_empty() {
                            tokens.push(Token::Text(raw_text.trim().to_string()));
                        }
                        // Stop processing (reached end of file)
                        // RUST FUNDAMENTAL: `break` here exits the outer tokenization loop entirely.
                        break;
                    }
                }
            }
        } else {
            // Non-tag character, add to text buffer
            text_buffer.push(ch);
            // Advance by character width (UTF-8 safe)
            index += ch.len_utf8();
        }
    }

    // Emit any remaining text in buffer
    // RUST FUNDAMENTAL: Parsers often do one final flush after the main loop to emit any buffered data that did not end with a delimiter.
    if !text_buffer.is_empty() {
        // Collapse consecutive whitespace
        let collapsed = collapse_whitespace(&text_buffer);
        // Only emit if non-whitespace content
        if !collapsed.trim().is_empty() {
            // Decode entities and create text token
            tokens.push(Token::Text(decode_entities(&collapsed)));
        }
    }

    // Return token stream
    // RUST FUNDAMENTAL: Returning the vector moves ownership out of the function with no element-by-element copy.
    tokens
}

// Collapse multiple consecutive whitespace into single space
fn collapse_whitespace(input: &str) -> String {
    // Initialize result string
    let mut result = String::new();
    // Track if previous character was whitespace
    // RUST FUNDAMENTAL: A simple state variable like this is a common way to write one-pass text normalization logic.
    let mut last_was_whitespace = false;

    // Iterate through each character in input
    // RUST FUNDAMENTAL: `input.chars()` iterates Unicode scalar values, not raw bytes.
    for ch in input.chars() {
        // Check if character is whitespace
        // RUST FUNDAMENTAL: Character classification helpers like `is_whitespace()` are methods on `char`.
        if ch.is_whitespace() {
            // Only emit space if previous wasn't whitespace
            if !last_was_whitespace {
                // Add single space
                result.push(' ');
                // Mark that we just added whitespace
                last_was_whitespace = true;
            }
        } else {
            // Non-whitespace character, add it directly
            result.push(ch);
            // Mark that last character wasn't whitespace
            last_was_whitespace = false;
        }
    }
    // Return collapsed string
    result
}

// Replace common HTML entities with their Unicode equivalents
fn decode_entities(input: &str) -> String {
    // Apply chain of replace operations for common entities
    // RUST FUNDAMENTAL: Each `.replace(...)` produces a new `String`, so this is simple and readable but not the most allocation-efficient strategy.
    input
        // Non-breaking space to regular space
        .replace("&nbsp;", " ")
        // Less than symbol
        .replace("&lt;", "<")
        // Greater than symbol
        .replace("&gt;", ">")
        // Ampersand
        .replace("&amp;", "&")
        // Double quote
        .replace("&quot;", "\"")
        // Single quote
        .replace("&apos;", "'")
        // Copyright symbol
        .replace("&copy;", "\u{00A9}")
        // Registered trademark symbol
        .replace("&reg;", "\u{00AE}")
        // Trademark symbol
        .replace("&trade;", "\u{2122}")
        // Bullet point
        .replace("&bull;", "\u{2022}")
        // Middle dot
        .replace("&middot;", "\u{00B7}")
        // En-dash
        .replace("&ndash;", "\u{2013}")
        // Em-dash
        .replace("&mdash;", "\u{2014}")
}

// Parse an opening tag into tag name and attributes
fn parse_open_tag(source: &str) -> TagToken {
    // Remove trailing slash and whitespace, convert to chars
    // RUST FUNDAMENTAL: `.peekable()` wraps an iterator so you can inspect the next item without consuming it.
    let mut chars = source.trim_end_matches('/').trim_end().chars().peekable();
    // Initialize tag name buffer
    let mut tag_name = String::new();

    // Extract tag name (everything until first whitespace)
    while let Some(ch) = chars.peek() {
        // Stop when we hit whitespace
        if ch.is_whitespace() {
            break;
        }
        // Append character to tag name
        // RUST FUNDAMENTAL: `peek()` yields a reference to the next character, so `*ch` dereferences it back to `char`.
        tag_name.push(*ch);
        // Move to next character
        chars.next();
    }

    // Skip whitespace between tag name and attributes
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        // Move past whitespace character
        chars.next();
    }

    // Collect remaining characters into string
    // RUST FUNDAMENTAL: `.collect::<String>()` gathers iterator items into an owned string because `String` implements `FromIterator<char>`.
    let rest = chars.collect::<String>();
    // Create tag token with name and parsed attributes
    TagToken {
        tag_name,
        // Parse attribute string into key-value pairs
        attributes: parse_attributes(&rest),
    }
}

// Check if tag should be parsed as raw text (no nested tags)
fn is_raw_text_tag(tag_name: &str) -> bool {
    // Match script and style tags that preserve content literally
    // RUST FUNDAMENTAL: `matches!` works well for tiny predicate helpers like this because it keeps the logic expression-sized.
    matches!(tag_name, "script" | "style")
}

// Check if tag is self-closing (void) with no content
fn is_void_tag(tag_name: &str) -> bool {
    // Match HTML void elements that have no closing tag
    matches!(
        tag_name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

// Find the position of '>' that closes a tag (respecting quotes)
fn find_tag_end(source: &str) -> Option<usize> {
    // Convert source to vector of chars for indexing
    // RUST FUNDAMENTAL: Collecting into `Vec<char>` makes per-character indexing easy, at the cost of an extra allocation.
    let chars: Vec<char> = source.chars().collect();
    // Current position in character vector
    let mut i = 0;
    // Track if we're inside quoted string and which quote type
    // RUST FUNDAMENTAL: `Option<char>` is a compact way to model parser state: either we are outside quotes (`None`)
    // or we are inside a quote delimited by a specific character (`Some('"')` or `Some('\'')`).
    let mut quote_char: Option<char> = None;

    // Loop through all characters
    while i < chars.len() {
        // Match character and current quote state
        match (chars[i], quote_char) {
            // Found double quote while not quoted, start quoted section
            ('"', None) => quote_char = Some('"'),
            // Found matching double quote, end quoted section
            ('"', Some('"')) => quote_char = None,
            // Found single quote while not quoted, start quoted section
            ('\'', None) => quote_char = Some('\''),
            // Found matching single quote, end quoted section
            ('\'', Some('\'')) => quote_char = None,
            // Found '>' while not in quoted section, return position
            // RUST FUNDAMENTAL: Returning `Some(i)` immediately exits the function with success.
            ('>', None) => return Some(i),
            // All other characters, do nothing
            _ => {}
        }
        // Move to next character
        i += 1;
    }

    // No closing '>' found
    None
}

// Parse attribute string into key-value pairs map
fn parse_attributes(source: &str) -> BTreeMap<String, String> {
    // Initialize result map for attributes
    let mut attributes = BTreeMap::new();
    // Convert source string to character vector
    // RUST FUNDAMENTAL: Like `find_tag_end`, this helper chooses indexed character access over a streaming iterator for simpler parsing logic.
    let chars = source.chars().collect::<Vec<_>>();
    // Current position in character vector
    let mut index = 0;

    // Main parsing loop
    while index < chars.len() {
        // Skip leading whitespace before attribute name
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }

        // Stop if we've reached end of string
        if index >= chars.len() {
            break;
        }

        // Find start of attribute name
        let start = index;
        // Scan until whitespace or '=' (end of attribute name)
        while index < chars.len() && !chars[index].is_whitespace() && chars[index] != '=' {
            index += 1;
        }

        // Skip if we didn't read any name characters (shouldn't happen)
        // RUST FUNDAMENTAL: Defensive parser checks like this help avoid getting stuck in infinite loops on malformed input.
        if start == index {
            index += 1;
            continue;
        }

        // Extract attribute name from characters
        // RUST FUNDAMENTAL: Slicing the `Vec<char>` and collecting builds a fresh owned `String` for the attribute name.
        let name = chars[start..index].iter().collect::<String>();

        // Skip whitespace after attribute name
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }

        // Parse attribute value (may be absent)
        let value = if index < chars.len() && chars[index] == '=' {
            // Skip past '=' character
            index += 1;
            // Skip whitespace after '='
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }

            // Check if we reached end of string (empty value)
            if index >= chars.len() {
                String::new()
            } else if chars[index] == '"' || chars[index] == '\'' {
                // Quoted value
                // Remember which quote character opened the value
                let quote = chars[index];
                // Skip opening quote
                index += 1;
                // Mark start of value content
                let value_start = index;
                // Scan until matching closing quote
                while index < chars.len() && chars[index] != quote {
                    index += 1;
                }
                // Extract value content between quotes
                let value = chars[value_start..index].iter().collect::<String>();
                // Skip closing quote if present
                if index < chars.len() {
                    index += 1;
                }
                // Return parsed value
                value
            } else {
                // Unquoted value
                // Mark start of value content
                let value_start = index;
                // Scan until whitespace (end of value)
                while index < chars.len() && !chars[index].is_whitespace() {
                    index += 1;
                }
                // Extract value content
                chars[value_start..index].iter().collect::<String>()
            }
        } else {
            // No '=' found, this is a boolean attribute
            // RUST FUNDAMENTAL: HTML boolean attributes like `hidden` or `disabled` are represented here as present keys with empty-string values.
            String::new()
        };

        // Store parsed attribute in map
        // RUST FUNDAMENTAL: Inserting the same key again would replace the old value, because maps keep at most one value per key.
        attributes.insert(name, value);
    }

    // Return completed attributes map
    attributes
}

// Test module for HTML parser (only compiled in test builds)
// RUST FUNDAMENTAL: `#[cfg(test)]` conditionally includes this module only when running tests.
#[cfg(test)]
mod tests {
    // Import Parser for testing
    use super::Parser;
    // Import DOM node types for assertions
    use crate::dom::{Node, NodePtr};
    // Import BTreeMap for creating expected attributes
    use std::collections::BTreeMap;

    // Helper function to create element nodes for testing
    fn element(tag: &str, children: Vec<NodePtr>) -> NodePtr {
        // Create element with tag, no attributes, and given children
        Node::element_with_attributes(tag, BTreeMap::new(), children)
    }

    // Test that parser correctly builds DOM tree from nested HTML
    // RUST FUNDAMENTAL: `#[test]` marks a function for the Rust test harness, which will discover and run it automatically.
    #[test]
    fn parses_nested_html_into_dom_tree() {
        // RUST FUNDAMENTAL: Tests usually build a small input, run the code under test, and compare against an expected output.
        let mut parser = Parser::new("<html><body><p>Hello</p><p>World</p></body></html>");
        let document = parser.parse_document();

        // RUST FUNDAMENTAL: `assert_eq!` compares values using `PartialEq` and prints both sides on failure.
        assert_eq!(
            document,
            Node::document(vec![element(
                "html",
                vec![element(
                    "body",
                    vec![
                        element("p", vec![Node::text("Hello")]),
                        element("p", vec![Node::text("World")]),
                    ],
                )],
            )])
        );
    }

    #[test]
    fn ignores_whitespace_only_text_nodes() {
        let mut parser = Parser::new("<div>\n  <p>Text</p>\n</div>");
        let document = parser.parse_document();

        assert_eq!(
            document,
            Node::document(vec![Node::element(
                "div",
                vec![Node::element("p", vec![Node::text("Text")])],
            )])
        );
    }

    #[test]
    fn keeps_script_contents_as_raw_text() {
        let mut parser = Parser::new("<script>if (a < b) { run(); }</script>");
        let document = parser.parse_document();

        assert_eq!(
            document,
            Node::document(vec![Node::element(
                "script",
                vec![Node::text("if (a < b) { run(); }")],
            )])
        );
    }

    #[test]
    fn keeps_mismatched_closing_tag_for_parent_recovery() {
        let mut parser = Parser::new("<div><span>t</div><p>x</p>");
        let document = parser.parse_document();

        assert_eq!(
            document,
            Node::document(vec![
                Node::element("div", vec![Node::element("span", vec![Node::text("t")])],),
                Node::element("p", vec![Node::text("x")]),
            ])
        );
    }

    #[test]
    fn treats_img_as_void_element() {
        let mut parser =
            Parser::new("<div><img src=\"cat.txt\" alt=\"sleepy cat\"><p>caption</p></div>");
        let document = parser.parse_document();

        // RUST FUNDAMENTAL: Test setup often constructs expected maps and structs explicitly so the assertion is precise.
        let mut img_attributes = BTreeMap::new();
        img_attributes.insert("alt".to_string(), "sleepy cat".to_string());
        img_attributes.insert("src".to_string(), "cat.txt".to_string());

        assert_eq!(
            document,
            Node::document(vec![Node::element(
                "div",
                vec![
                    Node::element_with_attributes("img", img_attributes, Vec::new()),
                    Node::element("p", vec![Node::text("caption")]),
                ],
            )])
        );
    }

    #[test]
    fn preserves_tag_attributes() {
        let mut parser = Parser::new(
            r#"<div id="app" class="shell main"><p data-role=hero hidden>Hello</p></div>"#,
        );
        let document = parser.parse_document();

        let mut div_attributes = BTreeMap::new();
        div_attributes.insert("class".to_string(), "shell main".to_string());
        div_attributes.insert("id".to_string(), "app".to_string());

        let mut p_attributes = BTreeMap::new();
        p_attributes.insert("data-role".to_string(), "hero".to_string());
        p_attributes.insert("hidden".to_string(), String::new());

        assert_eq!(
            document,
            Node::document(vec![Node::element_with_attributes(
                "div",
                div_attributes,
                vec![Node::element_with_attributes(
                    "p",
                    p_attributes,
                    vec![Node::text("Hello")],
                )],
            )])
        );
    }

    #[test]
    fn handles_quoted_attributes_with_special_characters() {
        let mut parser = Parser::new(
            r#"<a href="http://example.com?foo=bar>baz" title="Text with 'quotes' > inside">Link</a>"#,
        );
        let document = parser.parse_document();

        let mut a_attributes = BTreeMap::new();
        a_attributes.insert(
            "href".to_string(),
            "http://example.com?foo=bar>baz".to_string(),
        );
        a_attributes.insert(
            "title".to_string(),
            "Text with 'quotes' > inside".to_string(),
        );

        assert_eq!(
            document,
            Node::document(vec![Node::element_with_attributes(
                "a",
                a_attributes,
                vec![Node::text("Link")],
            )])
        );
    }

    #[test]
    fn handles_json_in_data_attributes() {
        let mut parser = Parser::new(r#"<div data-config='{"key":"value","num":>0}'></div>"#);
        let document = parser.parse_document();

        let mut div_attributes = BTreeMap::new();
        div_attributes.insert(
            "data-config".to_string(),
            "{\"key\":\"value\",\"num\":>0}".to_string(),
        );

        assert_eq!(
            document,
            Node::document(vec![Node::element_with_attributes(
                "div",
                div_attributes,
                vec![],
            )])
        );
    }
}
