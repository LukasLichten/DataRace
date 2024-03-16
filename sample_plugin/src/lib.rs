use datarace_plugin_api_wrapper::wrappers::{DataStoreReturnCode, Message, PluginHandle, Property};
use datarace_plugin_api_wrapper::api;

// TODO: Implement get_plugin_name and free_plugin_name
datarace_plugin_api_wrapper::macros::free_string!();

#[no_mangle]
pub extern "C" fn get_plugin_description() -> datarace_plugin_api_wrapper::reexport::PluginDescription {
    datarace_plugin_api_wrapper::reexport::PluginDescription {
        id: 2,
        name: std::ffi::CString::new("sample_plugin").expect("string is string").into_raw(),
        version: [0,0,1],
        api_version: 0,
    }
}

// This generates the extern func, while also wrapping the types
datarace_plugin_api_wrapper::macros::init_fn!(handle_init);


// this function handles the init
// it takes a PluginHandle
fn handle_init(handle: PluginHandle) -> Result<(),String> {
    match api::create_property(&handle, "Test", Property::Int(5)) {
        DataStoreReturnCode::Ok => {
            // let v = api::get_property_value(&handle, &prop_handle).unwrap();
            // api::log_info(&handle, format!("{}", match v { Property::Int(i) => i.to_string(), _ => "NAN".to_string() }));
        },
        e => api::log_error(&handle, e)
    };

    match api::generate_property_handle(&handle, "sample_plugin.Test") {
        Ok(prop_handle) => {
            api::subscribe_property(&handle, &prop_handle);

            api::update_property(&handle, &prop_handle, Property::Int(1));


            // let v = api::get_property_value(&handle, &prop_handle).unwrap();
            // api::log_info(&handle, format!("{}", match v { Property::Int(i) => i.to_string(), _ => "NAN".to_string() }));
            //
            // let code = api::delete_property(&handle, prop_handle);
            // if code != DataStoreReturnCode::Ok {
            //     api::log_error(&handle, code);
            // } else {
            //     api::log_info(&handle, "Property succesfully deleted");
            // }
        },
        Err(e) => api::log_error(&handle, e)
    };

    Ok(())
}

datarace_plugin_api_wrapper::macros::update_fn!(handle_update);

// this function deal with messages during runtime
fn handle_update(handle: PluginHandle, msg: Message) -> Result<(), String> {
    match msg {
        Message::Update(prop_handle, value) => {
            // api::log_info(&handle, format!("{}", match value { Property::Int(i) => i.to_string(), _ => "NAN".to_string() }));
            
            if let Property::Int(i) = value {
                // Usually you won't subscribe to your own property (and then update it), so you have to store and
                // source your prop handle
                api::update_property(&handle, &prop_handle, Property::Int(i + 1));

                // if i > 400_000 {
                //     api::delete_property(&handle, prop_handle);
                // }
            }
        },
        Message::Removed(_prop_handle) => {
            api::log_info(&handle, "Property reached 400,000 and got deleted");
        },
        _ => ()
    }

    Ok(())
}
