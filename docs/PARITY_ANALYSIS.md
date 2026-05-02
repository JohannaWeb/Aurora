# Aurora Local vs Public Repository Parity Analysis

Generated: 2026-04-04  
Local: `/home/johanna/projects/Bastion/projects/Aurora` (7,990 LOC)  
Public: `https://github.com/JohannaWeb/Aurora` (5,057 LOC)

## Executive Summary

The local Aurora version is a **production-oriented evolution** of the public repository, adding professional-grade features for typography, GPU acceleration, and JavaScript execution. Both versions share the same core architecture (DOM → Style → Layout → Paint pipeline), but diverge significantly in implementation strategy and scope.

| Aspect | Public | Local | Status |
|--------|--------|-------|--------|
| **JavaScript** | Custom parser (946 LOC) | Boa engine integration (292 LOC) | Different approach |
| **Typography** | Simple character-based | Pre-baked atlas + TrueType shaping | Enhanced |
| **Rendering** | CPU-only pixel push | GPU-accelerated (wgpu/vello) | Enhanced |
| **Dependencies** | 3 crates | 16 crates | 13x more complex |
| **Code Comments** | Minimal | Comprehensive (30+ Rust fundamentals) | Enhanced |

---

## File Structure Comparison

### Shared Core Modules (9 files)

```
css.rs      (883 → 1,190 lines, +307)  CSS cascade + specificity rules
dom.rs      (74 → 255 lines, +181)     Rc<RefCell<>> DOM tree
fetch.rs    (454 → 680 lines, +226)    HTTP + comprehensive errors
html.rs     (384 → 762 lines, +378)    HTML tokenization + parsing
layout.rs   (941 → 1,739 lines, +798)  Box model + positioning engine
main.rs     (83 → 562 lines, +479)     Entry point + module organization
paint.rs    (614 → 715 lines, +101)    CPU rasterization to framebuffer
style.rs    (308 → 440 lines, +132)    CSS cascade application
window.rs   (370 → 511 lines, +141)    Event loop + display management
```

### Local-Only Advanced Modules (4 files, 1,135 LOC)

```
js_boa.rs   (292 lines)     Boa JavaScript engine FFI integration
atlas.rs    (288 lines)     Glyph atlas texture packing (guillotine algorithm)
font.rs     (300 lines)     TrueType font loading + rustybuzz shaping
gpu_paint.rs (255 lines)    GPU acceleration via wgpu + vello
```

### Public-Only Module (1 file, 946 LOC)

```
js.rs       (946 lines)     Custom JavaScript lexer/parser/evaluator
```

---

## Architecture Comparison

### 1. JavaScript Engine Strategy

#### Public: Custom Parser Approach

**Design:**
```
Text → Lexer (tokenize) → Token stream
                        ↓
                      Parser
                        ↓
        AST: Program { statements }
                        ↓
                     Evaluator
```

**Key Types:**
- `Token` enum: Identifier, Number, String, Keyword, Operator, etc.
- `Keyword` enum: var, let, const, function, if, else, return, etc.
- `Statement` enum: Expression, VariableDeclaration, FunctionDeclaration, IfStatement, etc.
- `Expr` enum: Number, String, Identifier, Binary, Call, etc.

**Characteristics:**
- 946 lines implementing lexer, parser, and basic evaluator
- Full control over language features
- Educational value (demonstrates parser construction)
- No external JS dependencies
- Limited to implemented features

#### Local: Boa Engine Delegation

**Design:**
```
JavaScript code → Boa Context (FFI)
                        ↓
        JS execution with full ES6+ support
                        ↓
DOM Node Registry (JS object ↔ Rust pointer mapping)
```

**Key Components:**
- `BoaRuntime`: Wraps JavaScript context + DOM registry
- `NodeRegistry`: Maps JS object IDs to Rust `NodePtr`
- `NodeCapture` / `DocCapture`: Closure captures for garbage collection
- Native functions: `getElementById`, `createElement`, `appendChild`, `setAttribute`
- Polyfills: XMLHttpRequest, console.log, window/document/navigator globals

**Characteristics:**
- 292 lines integrating production-grade JS engine
- Full ES6+ language support (let, const, arrow functions, promises, etc.)
- Automatic garbage collection via Boa's marking system
- Better web compatibility (XMLHttpRequest, proper global objects)
- Smaller footprint in source code

**Trade-offs:**
```
Public  → Educational + minimal dependencies
Local   → Production-ready execution + web compatibility
```

---

### 2. Text Rendering Strategy

#### Public: Minimal Implementation

- No font module
- Text measurement: `width = char_count * font_size`
- No glyph rasterization
- Simple but limited

#### Local: Professional Typography

**Components:**

1. **Font Loading (font.rs)**
   - `AtlasBuilder::build()`: Pre-rasterizes ASCII + extended ASCII (256 glyphs)
   - `get_ab_font()`: ab_glyph for rasterization
   - `get_font_face()`: rustybuzz for text shaping
   - Embedded TTF: `fonts/default.ttf`

2. **Atlas Management (atlas.rs)**
   - Pre-baked 1024×1024 texture with all glyphs
   - `AtlasPacker`: Guillotine algorithm for 2D bin packing
   - O(1) glyph lookup via character code
   - `GlyphMetrics`: position + dimensions in atlas

3. **Text Shaping (font.rs::layout_text_run)**
   - Proper glyph positioning with `x_advance`, `y_offset`
   - Supports ligatures, complex scripts via rustybuzz
   - `PositionedGlyph`: Individual glyph placement
   - `TextRun`: Complete laid-out sequence

**Performance Implications:**
- Atlas precomputation: O(1) during render
- No per-character rasterization during layout
- GPU texture mapping efficiency

**Trade-offs:**
```
Public  → Simpler, no typography dependencies
Local   → Professional text rendering, proper shaping, better UX
```

---

### 3. Painting/Rendering Strategy

#### Public: CPU Direct Framebuffer

```rust
Layout tree (boxes, text)
        ↓
  paint.rs::paint()
        ↓
Rasterize to Vec<u32>
        ↓
minifb::Window::update_with_buffer()
```

- Pure CPU-based rendering
- Direct pixel manipulation
- Uses minifb (3 kB library)

#### Local: GPU-Accelerated Pipeline

```rust
Layout tree
        ↓
paint.rs::paint() (CPU fallback)
    and/or
gpu_paint.rs::gpu_render() (GPU path)
        ↓
wgpu::Queue::submit(render_pass)
        ↓
vello::Scene rendered to surface
```

**GPU Stack:**
- **wgpu 23.0**: Cross-platform GPU API (OpenGL/Vulkan/Metal abstraction)
- **vello 0.7.0**: 2D vector graphics renderer
- **peniko 0.6**: GPU-friendly primitives (vectors, colors, transforms)

**Capabilities:**
- GPU-accelerated 2D rendering
- Opacity blending through tree
- Better performance for complex hierarchies
- Resolution-independent vector graphics

**Trade-offs:**
```
Public  → Understandable pixel pushing, minimal dependencies
Local   → Modern GPU performance, complex visual effects
```

---

## Dependency Analysis

### Public: Minimalist

```toml
[dependencies]
rustls = "0.23"              # TLS certificates
webpki-roots = "0.26"        # Root CA bundle
minifb = "0.25"              # Window + framebuffer
```

**Total**: 3 dependencies  
**Total crate graph**: ~15 transitive  
**Philosophy**: Minimal external dependencies, maximum control

### Local: Feature-Rich

```toml
[dependencies]
rustls = "0.23"              # TLS certificates
webpki-roots = "0.26"        # Root CA bundle
wgpu = "23.0"                # GPU rendering
winit = "0.30.5"             # Window + events (wgpu uses)
vello = "0.7.0"              # 2D GPU renderer
pollster = "0.3"             # Block on futures
flate2 = "1.0"               # Gzip decompression
boa_engine = "0.19"          # JavaScript engine
boa_gc = "0.19"              # GC traits for Boa
opus = { path = "../Opus" }  # Custom dependency
ab_glyph = "0.2"             # Font rasterization
ttf-parser = "0.21"          # TrueType parsing
rustybuzz = "0.18"           # Text shaping
peniko = "0.6"               # GPU primitives
image = "0.24"               # Image loading
anyhow = "1.0"               # Error handling
```

**Total**: 16 dependencies  
**Total crate graph**: ~80+ transitive  
**Philosophy**: Production features + type safety + correctness

### Dependency Growth Impact

| Category | Public | Local | Justification |
|----------|--------|-------|---------------|
| Runtime | minifb | wgpu/winit/vello | GPU acceleration |
| Language | none | boa_engine/boa_gc | JavaScript execution |
| Typography | none | ab_glyph/rustybuzz/ttf-parser | Professional text |
| Utilities | none | flate2/peniko/image | Compression + imaging |
| Local deps | none | opus | Custom Bastion dependency |

---

## Code Comment Enhancement

**Local version includes comprehensive Rust fundamentals documentation:**

### 30+ Documented Concepts

1. **Ownership System**: move semantics, consuming functions
2. **Borrowing**: immutable (`&T`), mutable (`&mut T`), lifetime elision
3. **Smart Pointers**: `Rc<T>`, `Arc<T>`, `RefCell<T>`, `OnceLock<T>`
4. **Interior Mutability**: runtime borrow checking, panic on double-mut
5. **Copy vs Clone**: bitwise copy vs deep clone semantics
6. **Pattern Matching**: `match`, `if let`, destructuring
7. **Enums**: algebraic data types with associated data
8. **Option<T>**: nullable values without null unsafety
9. **Result<T, E>**: error handling, `?` operator
10. **Traits**: type classes, impl blocks, generic bounds
11. **Lifetimes**: `'static`, `'a`, lifetime elision rules
12. **Generic Types**: `Vec<T>`, `HashMap<K, V>`, generic functions
13. **Collections**: Vec, HashMap, BTreeMap (ordering), VecDeque
14. **Closures**: capture modes, Fn/FnMut/FnOnce traits
15. **Iterators**: lazy evaluation, adapter pattern
16. **Builder Pattern**: fluent API for complex initialization
17. **Derived Traits**: #[derive(Debug, Clone, Default, etc.)]
18. **Type Aliases**: semantic naming without nominal types
19. **Modules**: namespace organization, visibility rules
20. **FFI Patterns**: Foreign Function Interface, unsafe boundaries
21. **Garbage Collection Traits**: Trace, Finalize for managed objects
22. **Trait Objects**: `dyn Trait`, runtime polymorphism
23. **Method Chaining**: fluent interfaces
24. **Row-Major Indexing**: 2D→1D array conversion
25. **String Types**: `String` vs `&str`, owned vs borrowed
26. **Panics**: expectation handling, panic propagation
27. **Type Inference**: Rust's automatic type deduction
28. **Attribute Macros**: #[allow], #[derive], #[test]
29. **Lazy Initialization**: OnceLock pattern for expensive setup
30. **Memory Safety**: ownership guarantees, no null pointers

Every comment pairs a code line with conceptual explanation, showing **where** each concept applies in real Aurora code.

---

## Architectural Decision Records

### Decision 1: JavaScript Implementation

**Context**: Need to execute JavaScript in embedded browser engine

**Options**:
- A. Custom parser (public version)
- B. Embed production JS engine (local version)

**Decision**: Both implemented; local version chose (B)

**Rationale**:
- Full ES6+ support
- Automatic GC integration
- Better web compatibility
- Manageable source code (292 vs 946 lines)

**Consequences**:
- ✅ Can run real JavaScript code
- ✅ Smaller source codebase
- ❌ External dependency (boa_engine)
- ❌ Complexity hidden in FFI

---

### Decision 2: Typography Approach

**Context**: Need to render text in the browser

**Options**:
- A. Simple character-count estimation (public)
- B. Pre-baked atlas + proper shaping (local)

**Decision**: Local version chose (B)

**Rationale**:
- Professional text quality
- Proper ligature support
- Efficient GPU texture access
- Standard font file support (TTF)

**Consequences**:
- ✅ Production-quality typography
- ✅ Proper script support (non-Latin)
- ❌ 3 additional dependencies
- ❌ Pre-computation overhead (once at startup)

---

### Decision 3: Rendering Backend

**Context**: Need to display pixels on screen

**Options**:
- A. CPU framebuffer push (public)
- B. GPU-accelerated rendering (local)

**Decision**: Local version implements both (fallback + GPU)

**Rationale**:
- Scalability to complex scenes
- Modern GPU utilization
- Vector graphics support (vello)
- Future animation capability

**Consequences**:
- ✅ Better performance at scale
- ✅ Vector-native rendering
- ❌ wgpu dependency complexity (~50 crates)
- ❌ Debugging harder (GPU-side operations)

---

## Reconciliation Strategies

### Strategy A: Preserve Dual Versions

**Approach**: Maintain both as separate targets

```toml
[features]
default = ["educational"]
educational = []              # Public version (minimal deps)
production = ["gpu", "js", "fonts"]  # Local version
```

**Cargo.toml**:
```toml
[dependencies]
rustls = "0.23"
webpki-roots = "0.26"
minifb = "0.25"               # Always for fallback

wgpu = { version = "23.0", optional = true }
vello = { version = "0.7.0", optional = true }
boa_engine = { version = "0.19", optional = true }
ab_glyph = { version = "0.2", optional = true }
rustybuzz = { version = "0.18", optional = true }
```

**Benefits**:
- Users pick their feature set
- Dependency management by use case
- Cleaner for learning (no Boa complexity)
- Future-proof for both approaches

**Drawbacks**:
- Conditional compilation increases complexity
- Feature interactions must be tested
- Larger total codebase

---

### Strategy B: Public Repository Merges Local

**Approach**: Gradually upstream local improvements

**Phases**:
1. Add font.rs + atlas.rs as optional features
2. Add gpu_paint.rs with feature flag
3. Keep both js.rs (parser) and js_boa.rs (engine)
4. Document design trade-offs in README

**Benefits**:
- Single source of truth
- Community benefits from improvements
- Fewer repos to maintain
- Clearer documentation

**Drawbacks**:
- Public repo bloat
- Feature interactions complex
- Harder to teach core concepts

---

### Strategy C: Document as Research/Production Split

**Approach**: Acknowledge intentional divergence

**Public (GitHub)**: Educational reference implementation
- Minimal dependencies
- Clear code flow
- Good for learning
- Full comments on fundamentals

**Local (Bastion)**: Production Aurora engine
- Professional features
- Optimized for real use
- Full stack (JS + fonts + GPU)
- Comprehensive comments

**Mutual Links**:
- Public README → Link to local version
- Local README → Link to public version
- Explain design philosophy differences

**Benefits**:
- No false choice between versions
- Both serve their purpose
- Cleaner mental model
- Easier documentation

**Drawbacks**:
- Community must know both exist
- Maintenance on two branches

---

## Recommended Action: Strategy C

**Rationale**:

1. **Versions Serve Different Goals**
   - Public: Learning Rust + browser architecture
   - Local: Production Aurora rendering engine

2. **Forcing Parity Would Harm Both**
   - Public loses clarity by adding optional features
   - Local loses focus by removing production features

3. **Documentation Solves the Problem**
   - Cross-reference clearly in READMEs
   - Explain design decisions in ADRs
   - Help users pick the right version

4. **Feasible Implementation**
   - Add cross-repo links
   - Write architectural comparison (this document)
   - Document feature matrix clearly
   - Mark versions as `educational` vs `production`

---

## Implementation Checklist

- [ ] Update public repository README with local version link
- [ ] Update local repository README with educational version link  
- [ ] Create feature comparison matrix in both READMEs
- [ ] Tag public release as `v0.1-educational`
- [ ] Tag local release as `v0.1-production`
- [ ] Add ARCHITECTURE.md to explain design choices
- [ ] Document each strategy option with trade-offs
- [ ] Link to this parity analysis from both READMEs

---

## Conclusion

Rather than forcing artificial parity, the two Aurora versions represent a **natural evolution**: the public repository serves as a clean teaching tool, while the local repository evolves toward a usable browser engine with production-grade features.

**Both are correct.** The question is not "which is better" but "which is appropriate for your goal?"

- **Learning Rust?** → Public version (clear, focused)
- **Building a browser?** → Local version (complete, optimized)

This analysis provides the framework for coexisting productively as two versions with a clear purpose statement.
