use datarace_plugin_api_wrapper::wrappers::{PluginHandle, Property, DataStoreReturnCode};
use datarace_plugin_api_wrapper::api;

// TODO: Implement get_plugin_name and free_plugin_name
datarace_plugin_api_wrapper::macros::plugin_name!(sample_plugin);

// This generates the extern func, while also wrapping the types
datarace_plugin_api_wrapper::macros::init_fn!(handle_init);


// this function handles the init
// it takes a PluginHandle
fn handle_init(handle: PluginHandle) -> Result<(),String> {
    match api::create_property(&handle, "Test", Property::Int(5)) {
        Ok(prop_handle) => {
            let v = api::get_property_value(&handle, &prop_handle).unwrap();
            api::log_info(&handle, format!("{}", match v { Property::Int(i) => i.to_string(), _ => "NAN".to_string() }));
        },
        Err(e) => api::log_error(&handle, e)
    };

    match api::get_property_handle(&handle, "sample_plugin.Test") {
        Ok(prop_handle) => {
            api::update_property(&handle, &prop_handle, Property::Int(1));

            let v = api::get_property_value(&handle, &prop_handle).unwrap();
            api::log_info(&handle, format!("{}", match v { Property::Int(i) => i.to_string(), _ => "NAN".to_string() }));

            let code = api::delete_property(&handle, prop_handle);
            if code != DataStoreReturnCode::Ok {
                api::log_error(&handle, code);
            } else {
                api::log_info(&handle, "Property succesfully deleted");
            }
        },
        Err(e) => api::log_error(&handle, e)
    };

    Ok(())
}
