// Module for DOM (Document Object Model) tree representation
// RUST FUNDAMENTAL: Modules are Rust's basic namespace system.
// `mod dom;` tells the compiler that this crate has a `dom` module, and Rust will load it from `dom.rs` or `dom/mod.rs`.
mod dom;

// Module for CSS stylesheet parsing and cascade rules
// RUST FUNDAMENTAL: Every `mod` adds another node to the crate's module tree.
// Items inside that module are then named with paths like `css::Stylesheet` or `dom::Node`.
mod css;

// Module for fetching HTML and other resources from network
// RUST FUNDAMENTAL: Items are private by default in Rust, including module contents.
// You expose them outward with `pub`, and `pub use` can re-export names from one module through another path.
mod fetch;

// Module for font handling and glyph metrics
// RUST FUNDAMENTAL: Visibility in Rust is explicit and path-based.
// `pub` makes an item visible outside the defining module, while `pub(crate)` keeps it visible only within this crate.
mod font;

// Module for texture atlasing and glyph rendering
// RUST FUNDAMENTAL: A module declaration does not execute code by itself.
// It just tells the compiler to compile another source file into this crate under the `atlas` namespace.
mod atlas;

// Module for HTML parsing and tokenization
// RUST FUNDAMENTAL: Browser-style trees often need multiple parts of the program to hold references to the same node.
// This project solves that with `Rc<RefCell<_>>`, which combines shared ownership with runtime-checked mutation.
mod html;

// Module for layout tree generation from styled nodes
// RUST FUNDAMENTAL: Recursive data structures are common in Rust, but they usually store owned child collections
// instead of embedding values directly forever. Each level owns its children, which makes the tree finite and well-structured.
mod layout;

// Module for painting layout boxes to framebuffer
// RUST FUNDAMENTAL: Splitting code into many modules is the normal Rust style for medium-sized projects.
// It keeps each file focused and makes imports explicit at call sites.
mod paint;

// Module for styling DOM with CSS rules
// RUST FUNDAMENTAL: This kind of "walk one structure and build another structure" code is a common systems-programming pattern.
// It resembles a visitor in spirit, even though Rust typically expresses it with plain functions and enums rather than OOP hierarchies.
mod style;

// Module for legacy JavaScript engine (disabled)
// RUST FUNDAMENTAL: Attributes like `#[allow(dead_code)]` attach metadata or compiler directives to items.
// Here it suppresses the warning that would normally fire for code that is currently unused.
mod js;

// Module for Boa JavaScript runtime integration
// RUST FUNDAMENTAL: Integrating another runtime or library usually means crossing some API boundary.
// In Rust that broad idea is often described as FFI or embedding, even when the other component is also written in Rust.
mod js_boa;

// Module for GPU window rendering and display
// RUST FUNDAMENTAL: Cross-platform libraries often hide platform differences behind traits and concrete backend types.
// User code then works against the common API instead of directly targeting X11, Wayland, Cocoa, and so on.
mod window;

// Module for GPU painting capabilities (currently unused)
// RUST FUNDAMENTAL: Rust warns aggressively about unused code because it often signals a bug or dead branch.
// When a module is intentionally parked for future use, `#[allow(dead_code)]` documents that intent to the compiler.
#[allow(dead_code)]
mod gpu_paint;

// Import CSS stylesheet type for managing style rules
// RUST FUNDAMENTAL: `use` creates a local name for an item that already exists somewhere else.
// `crate::...` starts the path from the current crate root, which is useful for absolute intra-project imports.
use crate::css::Stylesheet;

// Import HTML fetching function for loading remote documents
// RUST FUNDAMENTAL: In Rust, free functions are first-class items in the module system.
// That means they can be imported with `use` just like structs, enums, traits, and constants.
use crate::fetch::fetch_html;

// Import HTML parser for converting HTML strings to DOM
// RUST FUNDAMENTAL: Importing a type shortens call sites.
// After `use crate::html::Parser;`, code can write `Parser::new(...)` instead of the longer full path.
use crate::html::Parser;

// Import layout tree for spatial box calculations
// RUST FUNDAMENTAL: Rust resolves names through module paths, and only public items can be reached from outside their module.
// `use` does not copy anything; it only gives the current scope a convenient binding to that path.
use crate::layout::{LayoutBox, LayoutTree, ViewportSize};

// Import style tree for DOM nodes with applied styles
// RUST FUNDAMENTAL: When two types live in different modules, `use` keeps the local code from turning into long path soup.
use crate::style::StyleTree;

// Import PathBuf for file system path manipulation
// RUST FUNDAMENTAL: `std` is the standard library namespace.
// `PathBuf` is the owned, mutable path type, roughly analogous to how `String` is the owned form of string data.
use std::path::PathBuf;

// Import Rc for reference counting shared data
// RUST FUNDAMENTAL: `Rc<T>` means "reference-counted pointer".
// Multiple parts of the program can own clones of the same pointer, and the value is dropped only when the last clone goes away.
// `Rc` uses non-atomic counting, so it is cheaper but limited to single-threaded code; `Arc<T>` is the thread-safe version.
use std::rc::Rc;

// Import capability types and identity from Opus domain model
// RUST FUNDAMENTAL: External dependencies appear in code by their crate name.
// Cargo resolves those names using `Cargo.toml`, downloads/builds the crates, and links them into this project.
use opus::domain::{Capability, Identity};

// Import environment variable access
// RUST FUNDAMENTAL: `std::env` is the standard-library module for interacting with process environment data.
// Two common entry points are command-line arguments and environment variables.
use std::env;

// Import HashMap for the image cache
use std::collections::HashMap;

// Map from resolved image URL to decoded peniko ImageData (RGBA8 pixels).
// Shared between the layout pipeline and the GPU renderer.
pub type ImageCache = HashMap<String, peniko::ImageData>;

// Entry point function for Aurora browser
// RUST FUNDAMENTAL: `main` is the executable entry point for a Rust binary crate.
// This version returns `()`, the unit type, which is Rust's "no meaningful value" result and is similar to `void` in other languages.
fn main() {
    // Print startup message to stdout
    // RUST FUNDAMENTAL: The `!` means `println!` is a macro, not a normal function.
    // Macros operate on syntax and can expand into more code during compilation, which is how formatted printing works here.
    println!("Aurora: Starting up...");

    // Install rustls crypto provider for TLS operations
    // RUST FUNDAMENTAL: `::` is the path separator for modules, types, and associated items.
    // Chaining works because each call returns another value that the next call can operate on.
    rustls::crypto::ring::default_provider()
        // Configure as default crypto backend
        // RUST FUNDAMENTAL: Methods can take ownership of `self`, borrow it immutably, or borrow it mutably.
        // This method consumes the provider value and returns a `Result`, forcing the caller to handle success or failure.
        .install_default()
        // Panic if crypto provider fails to install
        // RUST FUNDAMENTAL: `expect(...)` extracts the success value from a `Result`.
        // If the result is `Err`, the program panics and prints the provided message, which is acceptable when failure is unrecoverable.
        .expect("Failed to install rustls crypto provider");
    // Print confirmation that crypto provider loaded
    println!("Aurora: Crypto provider installed.");

    // Create identity tuple with Johanna's capabilities and permissions
    // RUST FUNDAMENTAL: `let` creates a new binding, and Rust usually infers the type from the right-hand side.
    // Explicit type annotations are optional unless inference would be ambiguous or you want extra clarity.
    let identity = Identity::new(
        // Identity URI using decentralized identifier format
        // RUST FUNDAMENTAL: A string literal has type `&'static str`.
        // It is a borrowed string slice pointing at bytes baked into the program binary for the entire run of the program.
        "did:human:johanna",
        // Human-readable name
        "Johanna",
        // Mark this as a human identity type
        // RUST FUNDAMENTAL: Enum variants are namespaced by their enum type, so `IdentityKind::Human`
        // means "the `Human` variant of the `IdentityKind` enum".
        opus::domain::IdentityKind::Human,
        // Array of capabilities this identity has: network access and workspace read
        // RUST FUNDAMENTAL: `[T; N]` is a fixed-size array whose length is part of its type.
        // Arrays are usually stored inline, unlike `Vec<T>`, which is a heap-allocated growable buffer.
        [Capability::NetworkAccess, Capability::ReadWorkspace],
    );

    // Parse command-line arguments and environment variables into CLI options
    // RUST FUNDAMENTAL: An associated function like `CliOptions::from_env()` is often used as a smart constructor.
    // It centralizes all setup logic so `main` can work with one parsed value instead of raw strings and flags.
    let cli = CliOptions::from_env();

    // Fetch HTML content from URL or use demo HTML if no URL provided
    // RUST FUNDAMENTAL: `match` is an expression, not just a statement.
    // That means it produces a value, and every arm must evaluate to a compatible type.
    let html = match cli.input_url.as_deref() {
        // RUST FUNDAMENTAL: `Option<T>` encodes "maybe there is a value".
        // `Some(T)` means present, `None` means absent, and the type system forces callers to deal with both cases.
        // `.as_deref()` converts `Option<String>` into `Option<&str>` by borrowing the inner string instead of cloning it.
        Some(url) => match fetch_html(url, &identity) {
            // RUST FUNDAMENTAL: `Result<T, E>` is the standard error-handling type in Rust.
            // `Ok(T)` holds a success value and `Err(E)` holds an error value; this makes failures explicit instead of exceptional.
            Ok(html) => html,
            // If fetch fails, print error and exit with code 1
            Err(error) => {
                // RUST FUNDAMENTAL: `eprintln!` is the stderr counterpart to `println!`.
                // The `{...}` placeholders use formatting traits such as `Display` to render values as text.
                eprintln!("Failed to fetch {url}: {error}");
                // RUST FUNDAMENTAL: `std::process::exit` terminates the process immediately with the given status code.
                // By convention, `0` means success and non-zero values indicate different failure conditions.
                std::process::exit(1);
            }
        },
        // If no URL provided, use hardcoded demo HTML
        // RUST FUNDAMENTAL: Matching `None` explicitly is what makes the "missing URL" case visible in code instead of implicit.
        None => demo_html().to_string(),
    };

    // Clone URL argument for later use in stylesheet parsing
    // RUST FUNDAMENTAL: `.clone()` creates a second owned `Option<String>` with duplicated string contents if present.
    // In Rust, cloning is always explicit, which makes copying costs easier to spot during code review.
    let url_arg = cli.input_url.clone();

    // Set viewport width in pixels (width of rendering canvas)
    // RUST FUNDAMENTAL: Unsuffixed float literals start as an inferred floating-point type.
    // Rust will choose `f32` or `f64` based on surrounding context if it can; otherwise you may need an explicit suffix.
    let viewport_width = env_f32("AURORA_VIEWPORT_WIDTH").unwrap_or(1200.0);
    let viewport_height = env_f32("AURORA_VIEWPORT_HEIGHT").unwrap_or(900.0);
    // Parse HTML string into DOM tree structure
    // RUST FUNDAMENTAL: Borrowing with `&html` lets the parser read the string without taking ownership of it.
    // That means `html` can still be used later in this scope if needed.
    let dom = Parser::new(&html).parse_document();

    // Run JavaScript BEFORE layout so DOM mutations are visible to the layout pass.
    // Google and most modern pages inject their content via JS; running scripts first
    // means appendChild/setAttribute calls land in the shared Rc<RefCell<Node>> tree
    // before we freeze it into a LayoutTree.
    let scripts = extract_scripts(&dom);
    if !scripts.is_empty() {
        println!("Boa: Processing {} scripts...", scripts.len());
        let mut runtime = js_boa::BoaRuntime::new(Rc::clone(&dom));
        for (source, is_url) in scripts {
            let script_content = if is_url {
                if let Some(base) = url_arg.as_deref() {
                    match crate::fetch::resolve_relative_url(base, &source) {
                        Ok(full_url) => {
                            println!("Boa: Fetching external script: {}", full_url);
                            match crate::fetch::fetch_string(&full_url, &identity) {
                                Ok(content) => content,
                                Err(e) => {
                                    eprintln!("Failed to fetch script {}: {}", full_url, e);
                                    continue;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to resolve script URL {}: {}", source, e);
                            continue;
                        }
                    }
                } else {
                    continue;
                }
            } else {
                source
            };
            if let Err(e) = runtime.execute(&script_content) {
                eprintln!("JS Error: {}", e);
            }
        }
    }

    // Build stylesheet, style tree, and layout from the post-JS DOM.
    let mut stylesheet = Stylesheet::from_dom(&dom, url_arg.as_deref(), &identity);
    stylesheet.merge(Stylesheet::user_agent_stylesheet());
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let viewport = ViewportSize {
        width: viewport_width,
        height: viewport_height,
    };
    let content_viewport = ViewportSize {
        width: viewport_width,
        height: (viewport_height - window::BROWSER_CHROME_HEIGHT).max(1.0),
    };
    let layout = LayoutTree::from_style_tree_with_viewport(&style_tree, content_viewport);

    // Pre-fetch and decode all images referenced in the layout tree.
    let image_cache = load_images(layout.root(), url_arg.as_deref(), &identity);

    if cli.debug_dom {
        println!("{}", dom.borrow());
    }
    if cli.debug_style {
        println!("{style_tree}");
    }
    if cli.debug_layout {
        println!("{layout}");
    }

    // Initialize font atlas before rendering (forces glyph load)
    // Get metrics for sample character to populate atlas
    // RUST FUNDAMENTAL: Binding to `_` explicitly discards the returned value.
    // This is common when you want a function's side effect or lazy initialization but do not need the actual result.
    let _ = crate::font::get_glyph_metrics('A');

    // Check environment variables to determine rendering mode
    // Check if screenshot output path is specified in environment
    // RUST FUNDAMENTAL: `Result::is_ok()` collapses a `Result<T, E>` into a plain boolean answering only "did this succeed?".
    let has_screenshot = env::var("AURORA_SCREENSHOT").is_ok();
    // Check if headless mode is explicitly requested
    let is_headless = env::var("AURORA_HEADLESS").is_ok();
    // Check if X11 or Wayland display server is available
    // RUST FUNDAMENTAL: `||` is short-circuiting logical OR, so the right-hand side is evaluated only if the left-hand side is false.
    let has_display = env::var("DISPLAY").is_ok() || env::var("WAYLAND_DISPLAY").is_ok();

    // Decide whether to attempt window rendering
    // RUST FUNDAMENTAL: Boolean expressions can be grouped with parentheses for readability,
    // but Rust's precedence rules would still make this expression unambiguous without them.
    if has_screenshot || (!is_headless && has_display) {
        // Attempt to open interactive GPU window for rendering
        // RUST FUNDAMENTAL: `if let Err(error) = ...` both checks for failure and binds the error value in one step.
        let window_input = window::WindowInput {
            dom: Rc::clone(&dom),
            stylesheet,
            base_url: url_arg.clone(),
            identity: identity.clone(),
            viewport,
            layout,
            images: image_cache,
        };
        if let Err(error) = window::open(window_input) {
            // Print error if window creation fails
            eprintln!("Window disabled: {error}");
            // Suggest alternative output methods to user
            eprintln!("Set AURORA_SCREENSHOT=/path/output.png for file output or AURORA_HEADLESS=1 to skip window creation.");
        }
    } else if !is_headless && !has_display {
        // RUST FUNDAMENTAL: `else if` is just another conditional branch in the same chain.
        // The first branch whose condition is true runs, and the rest are skipped.
        // Print message if no display server detected (and not explicitly headless)
        eprintln!("No display server detected; skipping window creation.");
        // Suggest screenshot mode as alternative
        eprintln!("Set AURORA_SCREENSHOT=/path/output.png for file output.");
    } else {
        // Print message confirming headless mode is active
        eprintln!("Headless mode: skipping window");
    }
}

// Structure holding parsed command-line and environment options
// RUST FUNDAMENTAL: `#[derive(...)]` asks the compiler to generate standard trait implementations automatically.
// `Debug` enables developer-facing formatting, and `Clone` generates a `clone()` method that duplicates the struct field-by-field.
// `Copy` is intentionally not derived here because types containing `String` require explicit cloning rather than implicit bitwise copying.
#[derive(Debug, Clone)]
struct CliOptions {
    // Optional URL to fetch and render as input
    // RUST FUNDAMENTAL: Using `Option<String>` is Rust's replacement for nullable references or nullable strings.
    // The type itself tells readers and the compiler that this field may be absent.
    input_url: Option<String>,

    // Flag to print DOM tree after parsing
    // RUST FUNDAMENTAL: `bool` is a small primitive type and implements `Copy`,
    // so assigning it copies the value instead of moving ownership.
    debug_dom: bool,

    // Flag to print styled tree after CSS application
    // RUST FUNDAMENTAL: Small plain-data structs like this are common in Rust for bundling related configuration.
    debug_style: bool,

    // Flag to print layout tree after position calculations
    debug_layout: bool,
}

// Implementation of CliOptions parsing from command-line args
// RUST FUNDAMENTAL: Rust separates data definitions from behavior by putting methods in `impl` blocks.
// Associated functions like `from_env()` have no receiver, while methods take `self`, `&self`, or `&mut self`.
impl CliOptions {
    // Parse environment variables and command-line arguments
    // RUST FUNDAMENTAL: Rust has no special constructor keyword.
    // A plain associated function returning `Self` is the idiomatic constructor pattern.
    fn from_env() -> Self {
        // Skip program name, iterate over remaining arguments
        // RUST FUNDAMENTAL: `env::args()` returns an iterator, not a pre-built vector.
        // Iterators in Rust are lazy, so adapters like `.skip(1)` describe how to process values without allocating intermediate collections.
        let mut args = env::args().skip(1);

        // Start with no input URL
        // RUST FUNDAMENTAL: Bindings are immutable by default in Rust.
        // `mut` opts into mutation explicitly, which makes state changes easier to notice.
        let mut input_url = None;

        // Check for debug flags in environment variables
        // RUST FUNDAMENTAL: A binding can be initialized directly from any expression, including a function call.
        let mut debug_dom = env_flag("AURORA_DEBUG_DOM");

        // Check environment for debug style flag
        let mut debug_style = env_flag("AURORA_DEBUG_STYLE");

        // Check environment for debug layout flag
        let mut debug_layout = env_flag("AURORA_DEBUG_LAYOUT");

        // Process each command-line argument
        // RUST FUNDAMENTAL: `while let` combines looping with pattern matching.
        // This pattern keeps pulling items until `next()` returns `None`, at which point the loop ends naturally.
        while let Some(arg) = args.next() {
            // Match argument against known flags and values
            // RUST FUNDAMENTAL: Matching on `arg.as_str()` lets us branch on borrowed string slices instead of allocating new strings.
            // As with other matches, the compiler checks that the set of patterns is exhaustive.
            match arg.as_str() {
                // Fixture mode: load HTML from fixtures directory
                "--fixture" => {
                    // Get the next argument as fixture name
                    // RUST FUNDAMENTAL: `let ... else` is a destructuring form that requires a pattern to match.
                    // If the pattern fails, control immediately jumps to the `else` block, which is useful for early-exit parsing code.
                    let Some(name) = args.next() else {
                        // Error if no name provided after flag
                        eprintln!("Missing fixture name after --fixture");
                        // Exit with error code
                        std::process::exit(2);
                    };
                    // Construct file URL to fixture HTML
                    // RUST FUNDAMENTAL: `Some(...)` wraps a concrete value into the present case of `Option`.
                    input_url = Some(fixture_url(&name));
                }

                // Override debug_dom with command-line flag
                // RUST FUNDAMENTAL: Assignment to a `bool` replaces the previous value; because `bool` is `Copy`, no ownership issues are involved.
                "--debug-dom" => debug_dom = true,

                // Override debug_style with command-line flag
                "--debug-style" => debug_style = true,

                // Override debug_layout with command-line flag
                "--debug-layout" => debug_layout = true,

                // Reject unknown flags starting with --
                // RUST FUNDAMENTAL: This uses a match guard.
                // The arm is selected only if the pattern matches and the extra boolean condition is also true.
                other if other.starts_with("--") => {
                    eprintln!("Unknown flag: {other}");
                    std::process::exit(2);
                }

                // Treat other arguments as input URL
                // RUST FUNDAMENTAL: Match arms are checked top to bottom.
                // This final arm is effectively the fallback case for any argument that did not match a more specific pattern above.
                other => {
                    // RUST FUNDAMENTAL: `.to_string()` allocates a new owned `String` from a borrowed `&str`.
                    input_url = Some(other.to_string());
                }
            }
        }

        // Return populated CliOptions struct
        // RUST FUNDAMENTAL: Inside an `impl CliOptions`, the name `Self` is just shorthand for `CliOptions`.
        // Struct literal shorthand like `Self { input_url, debug_dom, ... }` reuses variable names when they match field names.
        // Rust requires every non-defaulted field to be initialized here.
        Self {
            input_url,
            debug_dom,
            debug_style,
            debug_layout,
        }
    }
}

// Check if environment variable is set to a truthy value
// RUST FUNDAMENTAL: Small helper functions keep logic local and make call sites easier to read.
// When a function has no side effects and depends only on its inputs, it is especially easy to test and reason about.
fn env_flag(name: &str) -> bool {
    // Get environment variable and check if it matches any true values
    // RUST FUNDAMENTAL: `matches!` is a macro that returns `true` when an expression fits a pattern and `false` otherwise.
    // It is useful when you want the power of pattern matching but only need a boolean answer.
    matches!(
        // Read environment variable and dereference for comparison
        // RUST FUNDAMENTAL: `env::var()` returns a `Result` because environment lookup can fail.
        // `.as_deref()` then converts the successful `String` into `&str` by borrowing it, which makes the later string-pattern match cheaper.
        env::var(name).as_deref(),
        // Match "1", "true" (any case), or "yes" (any case)
        // RUST FUNDAMENTAL: `|` combines patterns with logical OR semantics inside pattern matching.
        // Any one of these successful `Ok("...")` forms will satisfy the macro.
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn env_f32(name: &str) -> Option<f32> {
    env::var(name).ok()?.parse::<f32>().ok()
}

// Construct file:// URL to a test fixture HTML file
fn fixture_url(name: &str) -> String {
    // Get the Cargo manifest directory (project root) at compile time
    // RUST FUNDAMENTAL: `env!(...)` is a compile-time macro, not a runtime environment lookup.
    // It embeds the environment variable's value directly into the compiled binary as a string literal.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Add fixtures subdirectory
    // RUST FUNDAMENTAL: `PathBuf` is mutable, so methods like `.push(...)` modify the existing path in place.
    path.push("fixtures");
    // Add fixture name directory
    path.push(name);
    // Add index.html as the document to load
    path.push("index.html");
    // Format as file:// URL with absolute path
    // RUST FUNDAMENTAL: `format!(...)` is to strings what `println!(...)` is to output: it builds and returns an owned `String`.
    format!("file://{}", path.display())
}

// Return hardcoded HTML demo content for default rendering
fn demo_html() -> &'static str {
    // Return static string containing complete HTML document
    // RUST FUNDAMENTAL: A raw string literal `r#"... "#` treats most characters literally.
    // That makes it convenient for embedded HTML, CSS, or JSON where backslashes and quotes would otherwise need escaping.
    r#"
        <html>
            <head>
                <style>
                    h1 { color: #d26428; font-weight: bold; font-size: 48px; }
                    h2 { color: #2E3440; font-size: 32px; margin-top: 20px; }
                    p { font-size: 20px; }
                    code { color: #BF616A; font-size: 20px; }
                </style>
            </head>
            <body>
                <h1>Aurora Browser - Unicode & Symbol Test</h1>

                <h2>Basic Typography</h2>
                <p>This paragraph has multiple words that wrap across lines and includes <strong>bold text</strong> and <em>italic text</em> interspersed throughout to test proper spacing preservation.</p>

                <h2>Unicode Symbols</h2>
                <p>Weather: ☀ sun ☁ cloud ☂ umbrella ☃ snowman</p>
                <p>Stars: ★ filled ☆ empty ☇ comet</p>
                <p>Arrows: ← → ↑ ↓ ↔ ↕</p>
                <p>Math: ± × ÷ ≈ ≠ ≡ ∞</p>

                <h2>Box Drawing</h2>
                <p>─ horizontal bar │ vertical bar</p>
                <p>┌─┐ ├─┤ └─┘ box corners and tees</p>
                <p>┼ cross symbol</p>

                <h2>Special Characters</h2>
                <p>Symbols: © ® ° · – —</p>
                <p>Bullets: • ◦ ‣</p>
                <p>Degrees: 32° F = 0° C</p>

                <h2>Mixed Content</h2>
                <p>Temperature: 72° Status: ☀ Clear skies with ← wind from west.</p>
                <p>Box: ┌─────┐ filled │ with │ ├─────┤ lines └─────┘</p>
            </body>
        </html>
    "#
}

// Extract all script tags from DOM and return (content/src, is_external) tuples
// RUST FUNDAMENTAL: Return types can be nested composite types.
// `Vec<(String, bool)>` means "a growable list of 2-tuples", where each tuple stores a string plus a flag describing what that string means.
fn extract_scripts(node: &crate::dom::NodePtr) -> Vec<(String, bool)> {
    // Initialize result vector to collect script tuples
    // RUST FUNDAMENTAL: As with other vectors, this starts empty and grows as scripts are discovered during traversal.
    let mut scripts = Vec::new();

    // Define nested walk function to traverse DOM tree recursively
    // RUST FUNDAMENTAL: Rust allows defining helper functions inside other functions when the helper is only locally relevant.
    // This one does not close over outer variables; instead it receives `scripts` explicitly as `&mut Vec<_>`.
    fn walk(node: &crate::dom::NodePtr, scripts: &mut Vec<(String, bool)>) {
        // Borrow the node to access its data
        // RUST FUNDAMENTAL: `RefCell::borrow()` returns a smart borrow guard, `Ref<T>`.
        // Multiple shared borrows can coexist, but a mutable borrow would require exclusivity and would panic if another borrow is still active.
        let node_borrow = node.borrow();

        // Match on node type
        // RUST FUNDAMENTAL: `Ref<T>` implements deref, so `&*node_borrow` turns the borrow guard into a plain `&Node`.
        // That lets `match` work directly on the underlying enum value.
        match &*node_borrow {
            // If node is a script element
            // RUST FUNDAMENTAL: This arm matches only element nodes whose tag name also satisfies the guard condition.
            // Guards are a nice way to keep structural matching and boolean filtering in one place.
            crate::dom::Node::Element(el) if el.tag_name == "script" => {
                // Check if script has src attribute (external script)
                // RUST FUNDAMENTAL: Map lookup returns `Option<&V>` because the key might be missing.
                // `if let Some(src) = ...` handles only the present case without forcing a full `match`.
                if let Some(src) = el.attributes.get("src") {
                    // Add src URL as external script (true flag)
                    // RUST FUNDAMENTAL: The vector stores owned strings, but `get()` gave us `&String`.
                    // Cloning produces a new owned value we can move into the tuple stored by the vector.
                    scripts.push((src.clone(), true));
                } else {
                    // No src attribute means inline script
                    // Collect text content from script children
                    // RUST FUNDAMENTAL: `String::new()` creates an empty owned string buffer we can append to incrementally.
                    let mut content = String::new();

                    // Iterate through script tag's children
                    // RUST FUNDAMENTAL: Iterating over `&el.children` borrows the vector rather than taking ownership of it.
                    // That lets the element keep its child list intact while we inspect each child.
                    for child in &el.children {
                        // Borrow child to check its type
                        // RUST FUNDAMENTAL: The borrow is scoped to this loop iteration's `child_borrow` binding.
                        // Once that binding goes out of scope, the `RefCell` borrow guard is dropped automatically.
                        let child_borrow = child.borrow();

                        // If child is text node, append to content
                        // RUST FUNDAMENTAL: Pattern matching can destructure borrowed enum values too.
                        // Here `t` becomes a borrowed `String` from inside the `Text` variant.
                        if let crate::dom::Node::Text(t) = &*child_borrow {
                            // RUST FUNDAMENTAL: `push_str` copies bytes from a borrowed string slice into the existing `String` buffer.
                            content.push_str(t);
                        }
                    }

                    // Only add script if it has content
                    // RUST FUNDAMENTAL: The `!` prefix negates a boolean, so `!content.is_empty()` means "content is not empty".
                    if !content.is_empty() {
                        // Add script content as inline script (false flag)
                        // RUST FUNDAMENTAL: After `push`, ownership of `content` has moved into the vector,
                        // so this local variable can no longer be used unless it is rebuilt.
                        scripts.push((content, false));
                    }
                }
            }

            // If node is any other element, recurse into children
            // RUST FUNDAMENTAL: This is a more general element pattern that runs after the script-specific arm above.
            crate::dom::Node::Element(el) => {
                // Walk each child of this element
                // RUST FUNDAMENTAL: You do not need mutable access to iterate by shared reference.
                // `for child in &el.children` borrows each child pointer one at a time.
                for child in &el.children {
                    // Recursively process child nodes
                    // RUST FUNDAMENTAL: This is recursion: the function solves the big problem by calling itself on smaller subtrees.
                    // In tree code that is often the clearest approach, though extremely deep trees can make recursion a stack concern.
                    walk(child, scripts);
                }
            }

            // If node is document root, recurse into its children
            // RUST FUNDAMENTAL: Named-field enum variants are destructured with struct-like syntax, even though they live inside an enum.
            crate::dom::Node::Document { children } => {
                // Walk each top-level child
                // RUST FUNDAMENTAL: Borrowing a vector as a slice gives read-only access to its elements without transferring ownership.
                for child in children {
                    // Recursively process child nodes
                    walk(child, scripts);
                }
            }

            // Ignore text nodes (no scripts there)
            // RUST FUNDAMENTAL: The wildcard pattern means "all remaining cases".
            // It is useful when the value itself does not matter.
            _ => {}
        }
    }

    // Start recursive walk from the given node
    // RUST FUNDAMENTAL: Passing `&mut scripts` means the helper can keep appending into one shared output buffer.
    walk(node, &mut scripts);

    // Return collected scripts vector
    // RUST FUNDAMENTAL: Returning `scripts` moves ownership of the finished vector to the caller.
    // Because Rust uses move semantics by default, no extra copy of the vector contents is made here.
    scripts
}

// Walk the layout tree and collect the resolved URL for every image node.
fn collect_image_srcs(node: &LayoutBox, base_url: Option<&str>, out: &mut Vec<String>) {
    if let Some(src) = node.image_src() {
        let resolved = if let Some(base) = base_url {
            crate::fetch::resolve_relative_url(base, src).unwrap_or_else(|_| src.to_string())
        } else {
            src.to_string()
        };
        if !out.contains(&resolved) {
            out.push(resolved);
        }
    }
    for child in node.children() {
        collect_image_srcs(child, base_url, out);
    }
}

// Fetch and decode all images referenced by the layout tree.
// Returns a map from resolved URL to peniko::ImageData (RGBA8).
pub(crate) fn load_images(
    root: &LayoutBox,
    base_url: Option<&str>,
    identity: &Identity,
) -> ImageCache {
    let mut urls = Vec::new();
    collect_image_srcs(root, base_url, &mut urls);

    let mut cache = ImageCache::new();
    for url in urls {
        match crate::fetch::fetch_bytes(&url, identity) {
            Ok(bytes) => match image::load_from_memory(&bytes) {
                Ok(dyn_img) => {
                    let rgba = dyn_img.to_rgba8();
                    let width = rgba.width();
                    let height = rgba.height();
                    let pixels: Vec<u8> = rgba.into_raw();
                    let img_data = peniko::ImageData {
                        data: peniko::Blob::from(pixels),
                        format: peniko::ImageFormat::Rgba8,
                        alpha_type: peniko::ImageAlphaType::Alpha,
                        width,
                        height,
                    };
                    cache.insert(url, img_data);
                }
                Err(e) => eprintln!("Aurora: failed to decode image {url}: {e}"),
            },
            Err(e) => eprintln!("Aurora: failed to fetch image {url}: {e}"),
        }
    }

    cache
}
