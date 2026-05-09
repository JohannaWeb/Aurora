# Screenshot Renders

Refresh the bundled fixture renders with:

```bash
make all-renders
```

Current fixture render outputs:

- `google-homepage.png` from `fixtures/google-homepage` at `1338 x 786`
- `aurora-search.png` from `fixtures/aurora-search` at `1338 x 786`
- `demo.png` from `fixtures/demo` at `1200 x 900`
- `dynamic-reflow.png` from `fixtures/dynamic-reflow` at `900 x 620`
- `raf-reflow.png` from `fixtures/raf-reflow` at `900 x 620`

The dynamic reflow render drains ready timer, animation frame, and microtask
work before screenshot capture.
