use opus::domain::{Capability, Identity};

use super::FetchError;

pub(super) fn require_file_access(identity: &Identity) -> Result<(), FetchError> {
    if identity
        .default_capabilities
        .contains(&Capability::ReadWorkspace)
    {
        Ok(())
    } else {
        Err(FetchError::InvalidUrl(format!(
            "Identity {} lacks workspace.read capability",
            identity.did
        )))
    }
}

pub(super) fn require_network_access(identity: &Identity) -> Result<(), FetchError> {
    if identity
        .default_capabilities
        .contains(&Capability::NetworkAccess)
    {
        Ok(())
    } else {
        Err(FetchError::InvalidUrl(format!(
            "Identity {} lacks network.access capability",
            identity.did
        )))
    }
}
