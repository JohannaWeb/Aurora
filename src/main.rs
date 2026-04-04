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
                    .header { background-color: aurora-cyan; color: white; padding: 20px; margin: -20px -20px 20px -20px; }
                    .cat-box { background-color: sand; border: 4px solid ember; padding: 15px; margin: 15px 0; text-align: center; }
                    img { border: 2px solid pine; }
                </style>
            </head>
            <body>
                <div class="header">
                    <h1>Aurora Cat Browser</h1>
                </div>

                <div class="cat-box">
                    <h2>Behold, a Cat!</h2>
                    <img src="https://cataas.com/cat" width="400" height="300" alt="A cute cat from CATAAS">
                    <p>This image was fetched and decoded by Aurora.</p>
                </div>
            </body>
        </html>
    "#
}
