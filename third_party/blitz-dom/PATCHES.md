# Local Blitz DOM patch

Source: crates.io `blitz-dom`, at the version recorded in `UPSTREAM`. The upstream
license and repository are recorded in Cargo.toml. To move to a newer release, run
`scripts/update_forks.py update blitz-dom <version>` and resolve any conflicts it
reports; `scripts/update_forks.py diff blitz-dom` prints the patch below.

## Carried patches

`src/stylo.rs` (`selectors::Element::parent_element`) + `src/node/node.rs`
(`Node::is_synthetic_shadow_container`): selector matching sees through Aurora's
synthetic `<div data-aurora-shadow-root>` container, so ShadyDOM-scoped CSS such
as `ytd-page-manager > .ytd-page-manager` matches shadow content as the host's
direct children.

Regression coverage: `child_combinator_matches_through_synthetic_shadow_container`
in `src/blitz_document.rs`. Remove when Aurora's shadow trees no longer need the
synthetic container (roadmap Phase 4).

## Dropped in the 0.3.0-beta.4 update (fixed upstream)

- `src/layout/construct.rs`, `display: contents` flattening losing flex/grid
  items: upstream now recurses through contents nodes with the parent's wrap
  context.
- `src/layout/construct.rs` (`classify_flow_children`), `display: none` and
  `display: contents` children forcing an inline formatting root: upstream's
  classification skips `display: none` and classifies through contents nodes.
- `src/resolve.rs` (`anchor_fixed_position_boxes_to_viewport`), fixed boxes
  positioned against their layout parent: Taffy 0.15 hoists out-of-flow boxes to
  their containing block, so fixed boxes are laid out against the viewport.
  Aurora's `dom_node_layout_metrics` now walks `Node::containing_block()`.

Their regression tests still run and pass against upstream:
`shadow_root_does_not_add_a_layout_box` (`src/js_v8/mirror_mutation_tests.rs`),
`abspos_root_beside_display_none_siblings_spans_viewport`,
`display_contents_shadow_container_keeps_host_a_block_container`,
`fixed_box_is_anchored_to_viewport_not_parent` (`src/blitz_document.rs`).
