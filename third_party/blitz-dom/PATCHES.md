# Local Blitz DOM patch

Source: crates.io `blitz-dom` 0.3.0-alpha.5, copied from the locked dependency.
The upstream license and repository are recorded in Cargo.toml.

`src/layout/construct.rs`: preserve the parent formatting context when flattening
`display: contents` children. The previous recursion collected each grandchild's
children instead of its box, losing flex/grid items under a contents container.
Aurora's synthetic shadow roots need this to avoid adding a layout box.

Regression coverage: `shadow_root_does_not_add_a_layout_box` in
`src/js_v8/mirror_mutation_tests.rs`. Remove this fork when the dependency contains
the equivalent fix.

`src/layout/construct.rs` (`classify_flow_children`): `display: none` children
no longer keep a flow container's `all_inline` flag set, and `display: contents`
children are classified through rather than treated as inline. Previously a
`<body>` holding only `<script>`s next to an absolutely positioned app root, or a
shadow host whose only child is Aurora's `display: contents` shadow container,
became an inline formatting root: block children were laid out as inline boxes
and an abspos `left: 0; right: 0` root shrank to fit (YouTube's `ytd-app`).

`src/stylo.rs` (`selectors::Element::parent_element`) + `Node::is_synthetic_shadow_container`:
selector matching sees through Aurora's synthetic `<div data-aurora-shadow-root>`
container, so ShadyDOM-scoped CSS such as `ytd-page-manager > .ytd-page-manager`
matches shadow content as the host's direct children.

`src/resolve.rs` (`anchor_fixed_position_boxes_to_viewport`): `stylo_taffy` maps
`position: fixed` to `Absolute`, positioning fixed boxes against their layout
parent. A post-layout pass re-anchors fixed boxes with explicit insets to the
viewport (position only; sizes still resolve against the parent).

Regression coverage: `abspos_root_beside_display_none_siblings_spans_viewport`,
`display_contents_shadow_container_keeps_host_a_block_container`,
`child_combinator_matches_through_synthetic_shadow_container`,
`fixed_box_is_anchored_to_viewport_not_parent` in `src/blitz_document.rs`.
