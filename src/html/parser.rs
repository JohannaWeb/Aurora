use crate::dom::Node;

use super::classify::is_void_tag;
use super::tokenizer::tokenize;
use super::tokens::{TagToken, Token};

pub struct Parser<'a> {
    tokens: Vec<Token>,
    position: usize,
    #[allow(dead_code)]
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            tokens: tokenize(source),
            position: 0,
            source,
        }
    }

    pub fn parse_document(&mut self) -> crate::dom::NodePtr {
        let mut children = Vec::new();

        while !self.is_eof() {
            if let Some(node) = self.parse_node() {
                children.push(node);
            } else {
                self.position += 1;
            }
        }

        Node::document(children)
    }

    fn parse_node(&mut self) -> Option<crate::dom::NodePtr> {
        match self.peek()? {
            Token::Text(text) => {
                let text = text.clone();
                self.position += 1;
                Some(Node::text(text))
            }
            Token::OpenTag(tag) => {
                let tag = tag.clone();
                self.position += 1;

                if is_void_tag(&tag.tag_name) {
                    Some(Node::element_with_attributes(
                        tag.tag_name,
                        tag.attributes,
                        Vec::new(),
                    ))
                } else {
                    Some(self.parse_element(tag))
                }
            }
            Token::CloseTag(_) => None,
        }
    }

    fn parse_element(&mut self, tag: TagToken) -> crate::dom::NodePtr {
        let mut children = Vec::new();

        while let Some(token) = self.peek() {
            match token {
                Token::CloseTag(close_tag) if close_tag == &tag.tag_name => {
                    self.position += 1;
                    break;
                }
                Token::CloseTag(_) => break,
                _ => {
                    if let Some(node) = self.parse_node() {
                        children.push(node);
                    }
                }
            }
        }

        Node::element_with_attributes(tag.tag_name, tag.attributes, children)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn is_eof(&self) -> bool {
        self.position >= self.tokens.len()
    }
}
