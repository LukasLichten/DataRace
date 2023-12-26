use datarace_plugin_api_wrapper::reexport::PluginHandle;

// This generates the extern func, while also wrapping the types
datarace_plugin_api_wrapper::macros::init_fn!(handle_init);


// Well, wrapping the types when I am done implementing a wrapper
fn handle_init(handle: *mut PluginHandle) {
    datarace_plugin_api_wrapper::log_info(handle, "Watch me!".to_string());
}
