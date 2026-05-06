use std::path::PathBuf;

pub(crate) fn fixture_url(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("fixtures");
    path.push(name);
    path.push("index.html");
    format!("file://{}", path.display())
}

pub(crate) fn demo_html() -> &'static str {
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
