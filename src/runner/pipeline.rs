use crate::{CliOptions, Identity};

pub(crate) fn run_browser(cli: CliOptions, identity: Identity) {
    // ... existing code ...
    
    // After creating the runtime and laying out the document:
    runtime.fire_dom_content_loaded();
    runtime.clear_dirty_bits();
}
