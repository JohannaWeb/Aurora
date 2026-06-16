#[cfg(test)]
mod tests {
    use crate::js_v8::V8Runtime;
    use crate::html::Parser;
    use crate::js_engine::JsRuntime;

    fn dom_with_style(html: &str) -> crate::dom::NodePtr {
        Parser::new(html).parse_document()
    }

    #[test]
    fn v8_supports_document_stylesheets() {
        let html = r#"
            <html>
                <head>
                    <style>body { color: red; }</style>
                    <link rel="stylesheet" href="test.css">
                </head>
                <body></body>
            </html>
        "#;
        let mut runtime = V8Runtime::new(dom_with_style(html));

        // Test that document.styleSheets is defined and has the correct length.
        assert_eq!(
            runtime.eval_to_string("document.styleSheets.length"),
            Ok("2".to_string())
        );

        // Test item access.
        assert_eq!(
            runtime.eval_to_string("document.styleSheets[0] instanceof CSSStyleSheet"),
            Ok("true".to_string())
        );
        assert_eq!(
            runtime.eval_to_string("document.styleSheets[1].href"),
            Ok("test.css".to_string())
        );
    }

    #[test]
    fn v8_stylesheets_reflect_dynamic_changes() {
        let mut runtime = V8Runtime::new(dom_with_style("<html><body></body></html>"));

        assert_eq!(
            runtime.eval_to_string("document.styleSheets.length"),
            Ok("0".to_string())
        );

        runtime.execute(r#"
            const style = document.createElement('style');
            style.textContent = 'div { display: block; }';
            document.head.appendChild(style);
        "#).unwrap();

        // This should ideally reflect the change.
        // Current implementation in v8_post.js uses a `synced` flag that prevents re-syncing.
        assert_eq!(
            runtime.eval_to_string("document.styleSheets.length"),
            Ok("1".to_string())
        );
    }
}
