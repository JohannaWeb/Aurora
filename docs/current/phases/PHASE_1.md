# Phase 1 — HTML Parser (html5ever)

**Status: Complete ✅**

## Work items

- [x] Add `html5ever` and `markup5ever` to `Cargo.toml`
- [x] Implement `TreeSink` on `crate::dom::NodePtr`
- [x] Replace `Parser::parse_document` with html5ever
- [x] Delete hand-rolled tokenizer/state machine (`tokenizer.rs`, `tag_parsing.rs`, `tokens.rs`, `text.rs`, `classify.rs`)
- [x] Remove duplicate `decode_entities` from `src/layout/inline_text.rs` — html5ever decodes at tokenizer level
- [x] Add quirks-mode flag plumbed from html5ever
- [x] Test: foster parenting, adoption agency, `<textarea>` RCDATA

## Outcome

Spec-compliant HTML5 parsing. Closes P0 #1. Handles foster parenting, formatting elements, foreign content, error recovery, DOCTYPE quirks mode.
