use datarace_plugin_api_wrapper::wrappers::{PluginHandle, Property};
use datarace_plugin_api_wrapper::api;

// TODO: Implement get_plugin_name and free_plugin_name
datarace_plugin_api_wrapper::macros::plugin_name!(sample_plugin);

// This generates the extern func, while also wrapping the types
datarace_plugin_api_wrapper::macros::init_fn!(handle_init);


// this function handles the init
// it takes a PluginHandle
fn handle_init(handle: PluginHandle) -> Result<(),String> {
    match api::create_property(&handle, "Test", Property::Int(5)) {
        Ok(_prop_handle) => api::log_info(&handle, "Successfully created a property"),
        Err(e) => api::log_error(&handle, e)
    }

    Ok(())
}
