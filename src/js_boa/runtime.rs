use super::*;

pub struct BoaRuntime {
    context: Context,
    #[allow(dead_code)]
    document: NodePtr,
    #[allow(dead_code)]
    registry: NodeRegistry,
}

impl BoaRuntime {
    pub fn new(document: NodePtr) -> Self {
        let mut context = Context::default();
        let registry = NodeRegistry::new();

        install_globals(&mut context, &document, &registry);
        install_dom_constructors(&mut context);
        install_document(&mut context, &document, &registry);
        install_observers(&mut context);
        install_xhr_and_fetch(&mut context);

        Self {
            context,
            document,
            registry,
        }
    }

    pub fn execute(&mut self, script: &str) -> JsResult<JsValue> {
        self.context.eval(Source::from_bytes(script))
    }
}
