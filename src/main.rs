mod dom;
mod css;
mod fetch;
mod html;
mod layout;
mod paint;
mod style;
mod js;
mod window;

use crate::css::Stylesheet;
use crate::fetch::fetch_html;
use crate::html::Parser;
use crate::layout::LayoutTree;
use crate::style::StyleTree;
use std::env;

fn main() {
    let html = match env::args().nth(1) {
        Some(url) => match fetch_html(&url) {
            Ok(html) => html,
            Err(error) => {
                eprintln!("Failed to fetch {url}: {error}");
                std::process::exit(1);
            }
        },
        None => demo_html().to_string(),
    };

    let dom = Parser::new(&html).parse_document();
    let stylesheet = Stylesheet::from_dom(&dom);
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 800.0);

    window::open(&layout);
}

fn demo_html() -> &'static str {
    r#"
        <html>
            <head>
                <style>
                    body { background-color: mist; color: coal; margin: 20px; }
                    h1 { color: ember; font-size: 24px; margin-bottom: 10px; }
                    h2 { color: coal; font-size: 18px; margin-top: 15px; }
                    .section { background-color: sand; border: 3px solid ember; padding: 15px; margin: 15px 0; }
                    .content { background-color: paper-white; padding: 10px; margin: 10px 0; }
                    .header { background-color: aurora-cyan; color: white; padding: 20px; margin: -20px -20px 20px -20px; }
                </style>
            </head>
            <body>
                <div class="header">
                    <h1>Welcome to the Aurora Web</h1>
                </div>

                <div class="section">
                    <h2>About This Browser</h2>
                    <div class="content">
                        <p>Aurora is a minimal browser engine, inspired by the simplicity of Web 1.0.</p>
                        <p>It renders HTML and CSS into a visual layout without JavaScript.</p>
                    </div>
                </div>

                <div class="section">
                    <h2>Features</h2>
                    <div class="content">
                        <p>HTML parsing with tags, classes, and ids</p>
                        <p>CSS selectors and style inheritance</p>
                        <p>Box model with margins, borders, and padding</p>
                        <p>Image placeholders and layout</p>
                    </div>
                </div>

                <div class="section">
                    <h2>Navigation</h2>
                    <div class="content">
                        <p>Home | About | Contact | Documentation</p>
                    </div>
                </div>
            </body>
        </html>
    "#
}
