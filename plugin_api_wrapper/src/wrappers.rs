
/// The handle for this plugin passed through into this plugin from the API
/// Used for call to the Plugin API
#[derive(Debug, Clone)]
pub struct PluginHandle {
    ptr: *mut crate::reexport::PluginHandle
}

impl PluginHandle {
    pub fn new(ptr: *mut crate::reexport::PluginHandle) -> PluginHandle {
        PluginHandle { ptr }
    }

    pub(crate) fn get_ptr(&self) -> *mut crate::reexport::PluginHandle {
        self.ptr
    }
}
