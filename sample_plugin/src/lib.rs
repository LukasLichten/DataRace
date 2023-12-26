use datarace_plugin_api_wrapper::wrappers::PluginHandle;
use datarace_plugin_api_wrapper::api;

// TODO: Implement get_plugin_name and free_plugin_name

// This generates the extern func, while also wrapping the types
datarace_plugin_api_wrapper::macros::init_fn!(handle_init);


// this function handles the init
// it takes a PluginHandle
fn handle_init(handle: PluginHandle) {
    api::log_info(handle, "Watch me!");
}
