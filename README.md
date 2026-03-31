# Aurora

<img width="803" height="973" alt="image" src="https://github.com/user-attachments/assets/e2adcefb-2e59-4c25-9b95-3de31ebce4d1" />

Aurora is an early-stage Rust browser engine experiment. The current slice is intentionally small:

- tokenize a narrow HTML subset
- build a simple DOM tree
- extract and parse a tiny CSS subset from `<style>` tags
- match tag, `.class`, `#id`, and descendant rules into computed styles
- build a style tree with basic color inheritance
- derive a block-oriented layout tree
- paint the result into a tiny text framebuffer
- print both structures from a CLI binary

## Run

```bash
cargo run
```

To fetch a page over the network:

```bash
cargo run -- http://example.com/
```

Current fetch support is intentionally small:

- `http://` and `https://`
- basic redirects
- remote images render as placeholders using `<img>` layout and alt text

## Test

```bash
cargo test
```

## Next steps

1. Add a real tokenizer state machine.
2. Add more inherited properties and better CSS value handling.
3. Improve layout with inline flow, wrapping, and margins.
4. Replace the text framebuffer with a real raster or window backend.
