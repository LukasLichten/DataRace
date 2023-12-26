/// Contains all functions of the api
pub mod api;

/// Contains wrappers around api data
pub mod wrappers;

/// Serves to reexport certain C structs for purposes such as building callback functions
pub mod reexport {
    pub use datarace_plugin_api_sys::PluginHandle;
}

/// For building callback functions simply
pub mod macros {
    pub use wrapper_macro::*;
}
