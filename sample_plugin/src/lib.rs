use datarace_plugin_api_wrapper::wrappers::{DataStoreReturnCode, Message, PluginHandle, Property, PropertyHandle};
use datarace_plugin_api_wrapper::api;

// This is requires to handle deallocating strings
datarace_plugin_api_wrapper::macros::free_string_fn!();

// Generates the required plugin description
datarace_plugin_api_wrapper::macros::plugin_descriptor_fn!("sample_plugin", 0, 0, 1);

// This generates the extern func, while also wrapping the types
datarace_plugin_api_wrapper::macros::init_fn!(handle_init);

const PROP_HANDLE: PropertyHandle = datarace_plugin_api_wrapper::macros::generate_property_handle!("sample_plugin.Test");

// this function handles the init
// it takes a PluginHandle
fn handle_init(handle: PluginHandle) -> Result<(),String> {
    let prop_name = "sample_plugin.Test";
    let runtime_prop_handle = api::generate_property_handle(prop_name).unwrap();
    let compiled_prop_handle = datarace_plugin_api_wrapper::macros::generate_property_handle!(" sample_plugin.test");

    assert_eq!(runtime_prop_handle, compiled_prop_handle, "these will be equal for the same (case insensetive) name");
    assert_eq!(runtime_prop_handle, PROP_HANDLE, "including those in consts you can also stored them in consts");


    match api::create_property(&handle, "Test", &PROP_HANDLE, Property::Int(5)) {
        DataStoreReturnCode::Ok => {
            // let v = api::get_property_value(&handle, &prop_handle).unwrap();
            // api::log_info(&handle, format!("{}", match v { Property::Int(i) => i.to_string(), _ => "NAN".to_string() }));
        },
        e => api::log_error(&handle, e)
    };


    api::subscribe_property(&handle, &PROP_HANDLE);

    api::update_property(&handle, &PROP_HANDLE, Property::Int(1));


    // let v = api::get_property_value(&handle, &prop_handle).unwrap();
    // api::log_info(&handle, format!("{}", match v { Property::Int(i) => i.to_string(), _ => "NAN".to_string() }));
    //
    // let code = api::delete_property(&handle, prop_handle);
    // if code != DataStoreReturnCode::Ok {
    //     api::log_error(&handle, code);
    // } else {
    //     api::log_info(&handle, "Property succesfully deleted");
    // }

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
