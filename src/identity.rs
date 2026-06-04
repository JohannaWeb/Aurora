#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Capability {
    NetworkAccess,
    ReadWorkspace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Identity {
    pub(crate) did: String,
    pub(crate) name: String,
    pub(crate) default_capabilities: Vec<Capability>,
}

impl Identity {
    pub(crate) fn new(
        did: impl Into<String>,
        name: impl Into<String>,
        default_capabilities: impl IntoIterator<Item = Capability>,
    ) -> Self {
        Self {
            did: did.into(),
            name: name.into(),
            default_capabilities: default_capabilities.into_iter().collect(),
        }
    }
}
