use datarace_plugin_api::wrappers::{DataStoreReturnCode, Message, PluginHandle, Property, PropertyHandle};

pub(crate) type PluginState = State;

// This is requires to handle deallocating strings
datarace_plugin_api::macros::free_string_fn!();

// Generates the required plugin description
// You have to pass in literals (at least so far, unfortunatly)
datarace_plugin_api::macros::plugin_descriptor_fn!("sample_plugin", 0, 1, 0);

// This generates the extern funcs, while also wrapping the types
// you pass in the two function names that handle init and update
// Optionally you pass in the statetype as the third value, which you will have to return out of
// the init handle function (which will then be stored into the state)
// But if you don't want the state automatically saved, you can save parse a boolean in as a forth
// value and turn it off (ideal if you want to spin up a worker thread in the init function)
// datarace_plugin_api::macros::generate_funcs!(handle_init, handle_update, PluginState);

const PROP_HANDLE: PropertyHandle = datarace_plugin_api::macros::generate_property_handle!("sample_plugin.Test");


// Allows you to store data between invocations
pub(crate) struct State {
    lock_count: std::sync::atomic::AtomicU64,
}

// This function handles the init
//
// it takes a PluginHandle and returns Result<PluginState,ToString>
// This means the state returned on Ok is automatically saved.
// If you don't want to automatically save the state (spin up worker threads during init),
// then set Return to Result<(),String>
//
// Err(String) does not have to be string, just be a Type implementing ToString.
// Returning Err will shutdown the plugin
#[datarace_plugin_api::macros::plugin_init]
fn handle_init(handle: PluginHandle) -> Result<PluginState,String> {
    let prop_name = "sample_plugin.Test";
    
    let runtime_prop_handle = datarace_plugin_api::api::generate_property_handle(prop_name).unwrap();
    let compiled_prop_handle = datarace_plugin_api::macros::generate_property_handle!(" sample_plugin.test");

    assert_eq!(runtime_prop_handle, compiled_prop_handle, "these will be equal for the same (case insensetive) name");
    assert_eq!(runtime_prop_handle, PROP_HANDLE, "including those in consts you can also stored them in consts");


    match handle.create_property("Test", PROP_HANDLE, Property::Int(5)) {
        DataStoreReturnCode::Ok => (),
        e => handle.log_error(e)
    };

    handle.subscribe_property(PROP_HANDLE);

    let array = datarace_plugin_api::wrappers::ArrayHandle::new(&handle, Property::from(3), 3).unwrap();
    array.set(&handle, 1, Property::from(2));
    array.set(&handle, 2, Property::from(1));
    // handle.log_info(Property::from(array.clone()).to_string());
    
    let _ = handle.create_property("extra", datarace_plugin_api::macros::generate_property_handle!("sample_plugin.extra"), Property::from(array));

    Ok(State { lock_count: std::sync::atomic::AtomicU64::default() })
    // Ok(())
}

// This function deals with messages during runtime
// it takes a PluginHandle and Message, and returns Result<(),ToString>
//
// Err(String) does not have to be string, just be a Type implementing ToString.
// Returning Err will shutdown the plugin
#[datarace_plugin_api::macros::plugin_update]
fn handle_update(handle: PluginHandle, msg: Message) -> Result<(), String> {
    match msg {
        Message::StartupFinished => {
            // This triggers after all init related Messages are processed, a good indication that
            // all properties have been created.
            // Can be used to spin up worker threads
            

            handle.log_info("Startup finished");

            if let Ok(extra) = handle.get_property_value(datarace_plugin_api::macros::generate_property_handle!("sample_plugin.extra")) {
                handle.log_info(format!("Extra is: {}", extra.to_string()));
            }
        },
        Message::OtherPluginStarted(id) => {
            // Informs us of the startup of another plugin
            // This allows us to do things like subscribe to it's properties,
            // or deeper interactions through PluginMessages

            handle.log_info(format!("We got informed of the startup of another plugin: {id}"))
        },
        Message::Lock => {
            // This message comes in to lock the plugin handle to perform some write (like creating
            // a Property). This means we need to stop performing any reads on the handle
            // till we are unlocked again.
            // So we need to stop/hold any seperate threads.
            // The lock applies after this function call returns
            
            // As this sample doesn't have a seperate thread currently we just log something instead
            handle.log_info("Received Lock");

        },
        Message::Unlock => {
            // The pluginloader has finished write operations (for now) and we can resume
            // computation
            
            // Again, sample does not have a seperate thread currently, so we log
    
            let state = datarace_plugin_api::macros::get_state!(handle).ok_or("No state :(".to_string())?;
            handle.log_info(format!("Received Unlock #{}", state.lock_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));

            let start = std::time::Instant::now();

            match handle.get_property_value(PROP_HANDLE) {
                Ok(val) => {
                    // This line currently prints three times, once 5, then the time messaurement
                    // twice
                    // This is due to a quirk in how we are running this code on unlock, and when
                    // on the first step of subscription it unlocks it will excute this, enqueue
                    // the property_type_change ahead of step 2 of the subscription.
                    // Nothing bad happens, we get an upt to date ValueContainer into the
                    // subscription.
                    // Even if it got enqueued ahead of the 3 step, except
                    // there it would need an extra cycle to update the ValueContainer.
                    // But it would stay locked through this cycle, so we would only get 1 print
                    handle.log_info(format!("Value is {}", val.to_string()));
                },
                Err(e) => {
                    handle.log_error(e);
                    return Ok(()); // We currently have no state, so we can't tell if this is the
                }
            }

            let res = handle.update_property(PROP_HANDLE, Property::Int(2));
            if res != DataStoreReturnCode::Ok {
                handle.log_error(res);
                return Ok(()); // We currently have no state, so we can't tell if this is the
            }

            match handle.get_property_value(PROP_HANDLE) {
                Ok(val) => {
                    handle.log_info(format!("Value is {}", val.to_string()));
                },
                Err(e) => {
                    handle.log_error(e);
                }
            }

            let later = std::time::Instant::now();

            // let res = api::change_property_type(&handle, &PROP_HANDLE, Property::Duration(i64::try_from((later-start).as_micros()).expect("impossible to be out of bounds")));
            let res = handle.change_property_type(PROP_HANDLE, Property::Str(format!("{}us", (later-start).as_micros())));
            match res {
                DataStoreReturnCode::Ok => {
                    handle.log_info("Changed");
                    handle.send_internal_msg(2);
                },
                _ => {
                    handle.log_error(res);
                }
            }
        },
        Message::Shutdown => {
            // Shutdown signal, so if we want to store some config, this would be a great place to
            // save it.
            // But it is of note that shutdown update is only send if the program is shutdown
            // properly, if your plugin failed a previous update and got unloaded that way, then it
            // won't be send

            handle.log_info("See You, Space Cowboy...");
            unsafe { datarace_plugin_api::macros::drop_state_now!(handle) }
        },
        Message::InternalMsg(msg) => {
            // Message from our plugin
            // Useful to communicate from a workerthread into the main thread

            handle.log_info(format!("Internal Message received: {msg}"));
        },
        Message::PluginMessagePtr { origin, ptr, reason } => {
            // This is a message containing a raw memory pointer.
            // Useful for deep interactions between plugins,
            // but requires extreme caution and care to be taken when implementing.
            //
            // If you aren't interest in using it you can ignore these messages,
            // technically if something sends you such a message it will leak memory,
            // but this is acceptable, as sending unsolicited pointers is rude (and bad practice)
            // anyway, so shouldn't happen
            
            let _ = (origin, ptr, reason); // Technically a memory leak, but who cares
        },
        Message::Unknown => {
            // Fallback, for when the plugin is used with a newer version of libdatarace with more
            // message types
            handle.log_error("Unknown Message Received!");
        }
    }

    Ok(())
}
