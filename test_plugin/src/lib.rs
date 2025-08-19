use datarace_plugin_api::{api, macros, wrappers::{ArrayHandle, DataStoreReturnCode, EventHandle, Message, PluginHandle, PluginSettingsLoadState, Property, PropertyHandle}};
use std::{sync::atomic::{AtomicBool, AtomicU64}, time::Duration};

// While you can use this test_plugin similar to the sample plugin for inspiration,
// this plugin is designed to aggressively fail whenever an error is encountered,
// and you should have more tolerance for those (and definitly avoid panics)

pub(crate) type PluginState = State;

macros::free_string_fn!();

macros::plugin_descriptor_fn!("test_plugin", 0, 1, 0);

const PROP_HANDLE: PropertyHandle = macros::generate_property_handle!("test_plugin.Test");
const EVENT_HANLDE: EventHandle = macros::generate_event_handle!("test_plugin.event");

macros::propertys_initor!{ test, "test_plugin",
    (GEN_PROP_HANDLE, "generated", "Macros Rock!"),
    (TEST_VISIBLE, "dashvis", Property::Int(1)),
    (GEN_ARRAY_HANDLE, "gen_array", [5.4; 12]),
    (NONE_HANDLE, "null", None),
    (TIME_HANDLE, "gne_time", Duration::from_secs(5)),
    (TO_BE_DELETED, "delete_me", 5.5),
    (COUNTER, "counter", 0),

    (UNFINISHED_TEST, "unfinished_test", 8),
}

// Allows you to store data between invocations
pub(crate) struct State {
    startup_complete: AtomicBool,
    array_handle: ArrayHandle,
    action_id: AtomicU64,
}

// Settings Handles
const SETTING_STD: PropertyHandle = macros::generate_property_handle!("test_plugin.test");
const SETTING_ARR: PropertyHandle = macros::generate_property_handle!("test_plugin.arr");
const SETTING_TRANS: PropertyHandle = macros::generate_property_handle!("test_plugin.transient");
const SETTING_INVALID: PropertyHandle = macros::generate_property_handle!("invalid.invalid");

#[datarace_plugin_api::macros::plugin_init]
fn handle_init(handle: PluginHandle, loadstate: PluginSettingsLoadState) -> Result<PluginState,String> {
    handle.log_info("Start Init");
    let prop_name = " test_plugIn.Test ";

    let runtime_prop_handle = api::generate_property_handle(prop_name).map_err(|e| e.to_string())?;
    let compiled_prop_handle = macros::generate_property_handle!(" test_plugin.test ");

    assert_eq!(runtime_prop_handle, compiled_prop_handle, "these will be equal for the same (case insensetive) name");
    assert_eq!(runtime_prop_handle, PROP_HANDLE, "including those in consts you can also stored them in consts");

    if matches!(api::generate_property_handle(".leadingdot.error"), Err(DataStoreReturnCode::ParameterCorrupted)) &&
        matches!(api::generate_property_handle("nodoterror"), Err(DataStoreReturnCode::ParameterCorrupted)) {
        return Err("PropertyHandle Generation Failure checks did not bounce as required".to_string());        
    }

    assert_eq!(Ok(GEN_PROP_HANDLE), api::generate_property_handle("test_plugin.generated"), "property initor handle missmatched runtime generated handle");

    handle.log_info("PropertyHandle Generation Test Successful");
    
    // Calling the function created by propertys_initor macro
    test(&handle)?;

    // Creating the Properties manually
    match handle.create_property("Test", PROP_HANDLE, Property::Int(5)) {
        // One way of doing error handling:
        DataStoreReturnCode::Ok => (),
        e => { return Err(e.to_string()); }
    };
    handle.subscribe_property(PROP_HANDLE).to_result().map_err(|e| e.to_string())?;

    handle.log_info("Property creation and subscription triggered successfully");



    // Creating an array, of size 3, with inital value 3, but we override [1] = 2 and [2] = 1
    let array = ArrayHandle::new(&handle, Property::from(3), 3).unwrap();
    for item in array.iter() {
        if item != Property::Int(3) {
            return Err("Value initialization incorrect".to_string());
        }
    }

    assert_eq!(array.get(4), None, "Out of bounds access should return none");

    array.set(&handle, 1, Property::from(2)).to_result().map_err(|e| e.to_string())?;
    array.set(&handle, 2, Property::from(1)).to_result().map_err(|e| e.to_string())?;

    assert_eq!(array.set(&handle, 4, Property::Int(5)), DataStoreReturnCode::DoesNotExist, "Out of bounds write should fail with DoasNotExist");
    assert_eq!(array.set(&handle, 2, Property::Float(2.2)), DataStoreReturnCode::TypeMissmatch, "Submitting a different type should fail with TypeMissmatch");
    // Unable to test not authenticated
    
    let arr_clone = array.clone();
    handle.create_property("arr", macros::generate_property_handle!("test_plugin.arr"), Property::from(array))
        .to_result().map_err(|e| e.to_string())?;

    assert_eq!(arr_clone.get(0), Some(Property::Int(3)), "Unexpected Value on index 0");
    assert_eq!(arr_clone.get(1), Some(Property::Int(2)), "Unexpected Value on index 1");
    assert_eq!(arr_clone.get(2), Some(Property::Int(1)), "Unexpected Value on index 2");
    

    handle.log_info("Array creation successful");

    // Creating an event
    let ev = api::generate_event_handle("test_plugin.event").map_err(|e| e.to_string())?;
    
    assert_eq!(ev, EVENT_HANLDE, "Missmatch between runtime generated event handle and compiletime");

    handle.create_event(ev);
    handle.subscribe_event(ev);

    handle.log_info("Event creation successfully triggered");

    // Plugin Settings test
    match loadstate {
        PluginSettingsLoadState::NoFile => {
            handle.log_info("No PluginSettings exists for this plugin, we will create one");
            let res = handle.save_plugin_settings();

            if !matches!(res, PluginSettingsLoadState::Loaded) {
                handle.log_error(format!("Failed to save initial PluginSettings: {}", res.to_string()));
            }
        },
        PluginSettingsLoadState::Loaded => {
            handle.log_info("PluginSettings loaded for this plugin, testing is values are present...");
            
            let res = std::panic::catch_unwind(|| {
                assert_eq!(handle.get_plugin_settings_property(SETTING_STD), Ok(Property::Int(-34)), "SETTING_STD loaded incorrectly");
                match handle.get_plugin_settings_property(SETTING_ARR) {
                    Ok(Property::Array(arr)) => {
                        assert_eq!(arr.get(0), Some(Property::from("Tester")), "SETTING_ARR index 0 missmatch");
                        assert_eq!(arr.get(1), Some(Property::from("Array")), "SETTING_ARR index 1 missmatch");

                        assert_eq!(arr.len(), 2, "SETTING_ARR size missmatch");

                    },
                    Err(e) => panic!("Reading SETTING_ARR returned an error {e}"),
                    Ok(v) => panic!("SETTING_ARR value incorrect {:?}", v)
                }
                assert_eq!(handle.get_plugin_settings_property(SETTING_TRANS), Err(DataStoreReturnCode::DoesNotExist), "SETTING_TRANS should not exist");
            });
            match res {
                Ok(_) => (),
                Err(_) => {
                    handle.log_error("Settings test failed, continuing Tests...");
                }
            }
        },
        PluginSettingsLoadState::FromOlderVersion(v) => {
            handle.log_error(format!("Loaded PluginSettings, the test plugin has been updated, previous version was {}.{}.{}", v[0], v[1], v[2]));
        },
        PluginSettingsLoadState::FromNewerVersion(v) => {
            handle.log_error(format!("Loaded PluginSettings, the test plugin has been downgraded, previous version was {}.{}.{}", v[0], v[1], v[2]));
        },
        PluginSettingsLoadState::FsError(e) => {
            handle.log_error(format!("Failed to load PluginSettings due to File System Error: {e}"));
        },
        PluginSettingsLoadState::JsonParseError(e) => {
            handle.log_error(format!("Failed to load PluginSettings due to Json Parsing Error: {e}"));
        },
        PluginSettingsLoadState::Unknown => {
            panic!("PluginSettings failed to load due to a unknown error");
        }
    }

    handle.log_info("Plugin Successfully Initiated");

    // Returning Ok, in this case with our state. As we didn't create it earlier, we create it here
    Ok(State { startup_complete: AtomicBool::new(false), array_handle: arr_clone, action_id: AtomicU64::new(u64::MAX) })
    // Ok(())
}

#[datarace_plugin_api::macros::plugin_update]
fn handle_update(handle: PluginHandle, msg: Message) -> Result<(), String> {

    match msg {
        Message::StartupFinished => {
            handle.log_info("Startup finished, beginning runtime tests");

            handle.log_info("Beginning Settings Test...");
            let settings_time = std::time::Instant::now();
            match handle.create_plugin_settings_property("test", SETTING_STD, Property::Float(2.5), false) {
                DataStoreReturnCode::Ok => (),
                DataStoreReturnCode::AlreadyExists => {
                    assert_eq!(handle.change_plugin_settings_property(SETTING_STD, Property::Float(2.5)), DataStoreReturnCode::Ok, "SETTING_STD already exists, and rewrite failed");
                },
                _ => panic!("Failed to create SETTING_STD")
            };
            assert_eq!(handle.get_plugin_settings_property(SETTING_STD), Ok(Property::Float(2.5)), "SETTING_STD did not get properly created");
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_STD), Some(false), "SETTING_STD should not be transient");

            if handle.get_plugin_settings_property(SETTING_TRANS).is_ok() {
                handle.log_error("SETTING_TRANS should not exist, but we are deleting it and continuing");
                assert_eq!(handle.delete_plugin_settings_property(SETTING_TRANS), DataStoreReturnCode::Ok, "Rouge SETTING_TRANS could not be deleted");
            }

            assert_eq!(handle.create_plugin_settings_property("transient", SETTING_TRANS, Property::Bool(false), true), DataStoreReturnCode::Ok, "Transient Property not created properly");
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_TRANS), Some(true), "SETTING_TRANS should be transient");

            let array = ArrayHandle::new(&handle, Property::from(""), 2).unwrap();
            assert_eq!(array.set(&handle, 0, Property::from("Tester")), DataStoreReturnCode::Ok, "Failed to set SETTING_ARR index 0");
            match handle.create_plugin_settings_property("arr", SETTING_ARR, Property::Array(array.clone()), false) {
                DataStoreReturnCode::Ok => (),
                DataStoreReturnCode::AlreadyExists => {
                    assert_eq!(handle.change_plugin_settings_property(SETTING_ARR, Property::Array(array.clone())), DataStoreReturnCode::Ok, "SETTING_ARR already exists, and rewrite failed");
                },
                _ => panic!("Failed to create SETTING_ARR")
            };
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_ARR), Some(false), "SETTING_ARR should not be transient");

            assert_eq!(handle.create_plugin_settings_property("invalid", SETTING_INVALID, Property::None, false), DataStoreReturnCode::NotAuthenticated, "Creating the invalid setting should fail");
            assert_eq!(handle.change_plugin_settings_property(SETTING_INVALID, Property::None), DataStoreReturnCode::NotAuthenticated, "Changing the invalid setting should fail");

            // array.set(&handle, 1, Property::from("Array"));

            handle.log_info("Settings Created, Performing Read...");
            assert_eq!(handle.get_plugin_settings_property(SETTING_STD), Ok(Property::Float(2.5)), "SETTING_STD read missmatch");
            assert_eq!(handle.get_plugin_settings_property(SETTING_TRANS), Ok(Property::Bool(false)), "SETTING_TRANS read missmatch");
            match handle.get_plugin_settings_property(SETTING_ARR) {
                Ok(Property::Array(arr)) => {
                    assert_eq!(arr.len(), 2, "SETTING_ARR size missmatch");
                    assert_eq!(arr.get(0), Some(Property::from("Tester")), "SETTING_ARR index 0 missmatch");
                    assert_eq!(arr.get(1), Some(Property::from("")), "SETTING_ARR index 1 missmatch");
                },
                Ok(e) => panic!("SETTING_ARR expected Array, got: {:?}", e),
                Err(e) => panic!("SETTING_ARR expected Array, got error: {}", e.to_string())
            }
            assert_eq!(handle.get_plugin_settings_property(SETTING_INVALID), Err(DataStoreReturnCode::NotAuthenticated), "Reading the invalid setting should fail");
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_INVALID), None, "Reading transience of the inalid setting should return none");

            handle.log_info("Settings Save and Reload test...");
            assert_eq!(handle.save_plugin_settings(), PluginSettingsLoadState::Loaded, "Unable to save settings");

            assert_eq!(array.set(&handle, 1, Property::from("Array")), DataStoreReturnCode::Ok, "Failed to set SETTING_ARR index 1");
            match handle.get_plugin_settings_property(SETTING_ARR) {
                Ok(Property::Array(arr)) => {
                    assert_eq!(arr.len(), 2, "SETTING_ARR size missmatch");
                    assert_eq!(arr.get(0), Some(Property::from("Tester")), "SETTING_ARR index 0 missmatch");
                    assert_eq!(arr.get(1), Some(Property::from("Array")), "SETTING_ARR index 1 missmatch");
                },
                Ok(e) => panic!("SETTING_ARR expected Array, got: {:?}", e),
                Err(e) => panic!("SETTING_ARR expected Array, got error: {}", e.to_string())
            }

            assert_eq!(handle.get_plugin_settings_property(SETTING_TRANS), Ok(Property::Bool(false)), "SETTING_TRANS re-read missmatch");
            assert_eq!(handle.change_plugin_settings_property(SETTING_TRANS, Property::Bool(true)), DataStoreReturnCode::Ok, "SETTING_TRANS change failed");
            assert_eq!(handle.get_plugin_settings_property(SETTING_TRANS), Ok(Property::Bool(true)), "SETTING_TRANS read-read-write-read missmatch");

            assert_eq!(handle.reload_plugin_settings(), PluginSettingsLoadState::Loaded, "Settings reload failed");

            assert_eq!(handle.get_plugin_settings_property(SETTING_TRANS), Err(DataStoreReturnCode::DoesNotExist), "SETTING_TRANS should not be available");
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_TRANS), None, "SETTING_TRANS transience check after reload should return none");

            match handle.get_plugin_settings_property(SETTING_ARR) {
                Ok(Property::Array(arr)) => {
                    assert_eq!(arr.len(), 2, "SETTING_ARR size missmatch post reload");
                    assert_eq!(arr.get(0), Some(Property::from("Tester")), "SETTING_ARR index 0 missmatch post reload");
                    assert_eq!(arr.get(1), Some(Property::from("")), "SETTING_ARR index 1 missmatch post reload");

                    assert_eq!(array.set(&handle, 1, Property::from("Unreachable")), DataStoreReturnCode::Ok, "Despite the write going to nowhere, this should still work");
                    assert_eq!(arr.get(1), Some(Property::from("")), "SETTING_ARR index 1 should remain unchanged");

                    assert_eq!(arr.set(&handle, 1, Property::from("Array")), DataStoreReturnCode::Ok, "SETTING_ARR final write to index 1 failed");
                    assert_eq!(arr.get(1), Some(Property::from("Array")), "SETTING_ARR index 1 missmatch post final write");
                },
                Ok(e) => panic!("SETTING_ARR expected Array post reload, got: {:?}", e),
                Err(e) => panic!("SETTING_ARR expected Array post reload, got error: {}", e.to_string())
            }
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_ARR), Some(false), "SETTING_ARR should not be transient");

            assert_eq!(handle.get_plugin_settings_property(SETTING_STD), Ok(Property::Float(2.5)), "SETTING_STD value missmatched after reload");
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_STD), Some(false), "SETTING_STD should not be transient");

            handle.log_info("Settings final checks...");
            assert_eq!(handle.delete_plugin_settings_property(SETTING_STD), DataStoreReturnCode::Ok, "SETTING_STD delete failed");
            assert_eq!(handle.create_plugin_settings_property("test", SETTING_STD, Property::Int(-34), true), DataStoreReturnCode::Ok, "SETTING_STD recreation should not fail");
            assert_eq!(handle.set_plugin_settings_property_transient(SETTING_STD, false), DataStoreReturnCode::Ok, "Set SETTING_STD Not Transient should not have failed");
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_STD), Some(false), "SETTING_STD should not be transient");

            assert_eq!(handle.create_plugin_settings_property("transient", SETTING_TRANS, Property::None, false), DataStoreReturnCode::Ok, "Transient Property not re-created properly");
            assert_eq!(handle.set_plugin_settings_property_transient(SETTING_TRANS, true), DataStoreReturnCode::Ok, "Set Setting Transient should not have failed");
            assert_eq!(handle.is_plugin_settings_property_transient(SETTING_TRANS), Some(true), "SETTING_TRANS should be transient");

            assert_eq!(handle.save_plugin_settings(), PluginSettingsLoadState::Loaded, "Settings 2nd save should not fail");
            assert_eq!(handle.reload_plugin_settings(), PluginSettingsLoadState::Loaded, "Settings 2nd reload should not fail");

            assert_eq!(handle.get_plugin_settings_property(SETTING_STD), Ok(Property::Int(-34)), "SETTING_STD loaded incorrectly");
            match handle.get_plugin_settings_property(SETTING_ARR) {
                Ok(Property::Array(arr)) => {
                    assert_eq!(arr.get(0), Some(Property::from("Tester")), "SETTING_ARR index 0 missmatch");
                    assert_eq!(arr.get(1), Some(Property::from("Array")), "SETTING_ARR index 1 missmatch");

                    assert_eq!(arr.len(), 2, "SETTING_ARR size missmatch");

                },
                Err(e) => panic!("Reading SETTING_ARR returned an error {e}"),
                Ok(v) => panic!("SETTING_ARR value incorrect {:?}", v)
            }
            assert_eq!(handle.get_plugin_settings_property(SETTING_TRANS), Err(DataStoreReturnCode::DoesNotExist), "SETTING_TRANS should not exist");

            handle.log_info(format!("Settings test successful, time: {}us", (std::time::Instant::now() - settings_time).as_micros()));

            handle.log_info("Performing Property tests...");
            assert_eq!(handle.get_property_value(UNFINISHED_TEST), Ok(Property::Int(8)), "Unfinished Test should have been on the initial value");

            assert_eq!(handle.get_property_value(GEN_PROP_HANDLE), Ok(Property::from("Macros Rock!")), "GEN_PROP_HANDLE initial value missmatch");
            assert_eq!(handle.get_property_value(TEST_VISIBLE), Ok(Property::Int(1)), "TEST_VISIBLE initial value missmatch");
            
            match handle.get_property_value(GEN_ARRAY_HANDLE) {
                Ok(Property::Array(a)) => {
                    assert_eq!(a.to_string(), "[5.4, 5.4, 5.4, 5.4, 5.4, 5.4, 5.4, 5.4, 5.4, 5.4, 5.4, 5.4]".to_string(), 
                        "Array to_string produced unexpected result");
                },
                Ok(i) => return Err(format!("Expected ArrayHandle for GEN, found {:?}", i)),
                Err(e) => return Err(format!("Expected ArrayHandle for GEN, failed with {}", e.to_string())),
            }

            assert_eq!(handle.get_property_value(NONE_HANDLE), Ok(Property::None), "None Property is not None");
            assert_eq!(handle.get_property_value(TIME_HANDLE), Ok(Property::from_millis(5000)), "Generated Time is incorrect");
            assert_eq!(handle.get_property_value(TO_BE_DELETED), Ok(Property::from(5.5)), "Generated Time is incorrect");
            
            assert_eq!(handle.get_property_value(PROP_HANDLE), Ok(Property::from(5)), "Test Prophandle missmatched");
            
            let arr_handle = match handle.get_property_value(macros::generate_property_handle!("test_plugin.arr")) {
                Ok(Property::Array(a)) => {
                    a
                },
                Ok(i) => return Err(format!("Expected ArrayHandle, found {:?}", i)),
                Err(e) => return Err(format!("Expected ArrayHandle, failed with {}", e.to_string())),
            };
            
            handle.log_info("Property Initial Read Test Successful");
            
            let state = datarace_plugin_api::macros::get_state!(handle).ok_or("No state :(".to_string())?;
            state.startup_complete.store(true, std::sync::atomic::Ordering::Relaxed);

            // But not a full guarantee that the memory is not corrupted
            handle.log_info("State Retrival successful");

            assert_eq!(arr_handle, state.array_handle, "These should point to the same array");
            arr_handle.set(&handle, 1, Property::Int(12));
            assert_eq!(arr_handle.get(1), Some(Property::Int(12)), "Value did not write properly");
            assert_eq!(state.array_handle.get(1), Some(Property::Int(12)), "Value did not write properly");

            handle.log_info("ArrayHandles match and writes successful");

            assert!(handle.update_property(GEN_PROP_HANDLE, Property::from("Write The Macros!")).is_ok(), "Writing String Property failed");
            assert!(handle.update_property(TEST_VISIBLE, Property::from(3)).is_ok(), "Writing Int Property failed");
            assert!(handle.update_property(TIME_HANDLE, Property::from(Duration::from_secs(2))).is_ok(), "Writing Time Property failed");
            assert!(handle.update_property(PROP_HANDLE, Property::Int(2)).is_ok(), "Writing PROP_HANDLE Property failed");
            assert!(handle.update_property(TO_BE_DELETED, Property::Float(3.2)).is_ok(), "Writing TO_BE_DELETED Property failed");


            assert_eq!(handle.update_property(GEN_PROP_HANDLE, Property::Bool(false)), DataStoreReturnCode::TypeMissmatch, "Type Missmatch test #1 did not fail");
            // From_string function will produce a string, even form an int which would be the correct type:
            assert_eq!(handle.update_property(TEST_VISIBLE, Property::from_string(4)), DataStoreReturnCode::TypeMissmatch, "Type Missmatch test #2 did not fail");
            assert_eq!(handle.update_property(TIME_HANDLE, Property::Float(3.5)), DataStoreReturnCode::TypeMissmatch, "Type Missmatch test #3 did not fail");
            assert_eq!(handle.update_property(PROP_HANDLE, Property::from_millis(-5)), DataStoreReturnCode::TypeMissmatch, "Type Missmatch test #4 did not fail");

            assert_eq!(handle.get_property_value(GEN_PROP_HANDLE), Ok(Property::from("Write The Macros!")), "GEN_PROP_HANDLE read after write failed");
            assert_eq!(handle.get_property_value(TEST_VISIBLE), Ok(Property::Int(3)), "TEST_VISIBLE read after write failed");
            assert_eq!(handle.get_property_value(TIME_HANDLE), Ok(Property::from_sec(2.0)), "TIME_HANDLE read after write failed");
            assert_eq!(handle.get_property_value(PROP_HANDLE), Ok(Property::Int(2)), "PROP_HANDLE read after write failed");
            assert_eq!(handle.get_property_value(TO_BE_DELETED), Ok(Property::Float(3.2)), "TO_BE_DELETED read after write failed");

            handle.log_info("Property Read-Write successful");

            assert!(handle.change_property_type(GEN_PROP_HANDLE, Property::Bool(true)).is_ok(), "GEN_PROP_HANDLE type change failed");
            assert!(handle.change_property_type(PROP_HANDLE, Property::Float(4.5)).is_ok(), "PROP_HANDLE type change failed");
            // Same type, should not fail and set the new value:
            assert!(handle.change_property_type(TEST_VISIBLE, Property::Int(2)).is_ok(), "TEST_VISIBLE type change failed");
            
            let arr_handle = ArrayHandle::new(&handle, Property::Float(5.4), 5).ok_or("Failed to create ArrayHandle".to_string())?;
            assert_eq!(handle.update_property(GEN_ARRAY_HANDLE, Property::Array(arr_handle.clone())), DataStoreReturnCode::TypeMissmatch, "Updating array via update_property must fail");
            assert!(handle.change_property_type(GEN_ARRAY_HANDLE, Property::Array(arr_handle)).is_ok(), "GEN_ARRAY_HANDLE type change failed");

            handle.log_info("Type Change Triggered Successfully");

            assert_eq!(handle.create_property("delete_me", TO_BE_DELETED, Property::Float(69.0)), DataStoreReturnCode::AlreadyExists, "TO_BE_DELETED still exists");
            assert!(handle.delete_property(TO_BE_DELETED).is_ok(), "Deleted should have succeeded");

            handle.log_info("Delete Triggered Successfully");

            decrease_unfinished_tests(&handle)?;

            handle.log_info("Waiting for lock & unlock...");
        },
        Message::OtherPluginStarted(_id) => {},
        Message::Lock => {},
        Message::Unlock => {
            let state = datarace_plugin_api::macros::get_state!(handle).ok_or("No state :(".to_string())?;
            if !state.startup_complete.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            handle.log_info("Unlock received, continuing tests...");

            assert_eq!(handle.get_property_value(GEN_PROP_HANDLE), Ok(Property::from(true)), "GEN_PROP_HANDLE read after type change failed");
            assert_eq!(handle.get_property_value(TEST_VISIBLE), Ok(Property::Int(2)), "TEST_VISIBLE read after type change failed");
            assert_eq!(handle.get_property_value(PROP_HANDLE), Ok(Property::from(4.5)), "PROP_HANDLE read after type change failed");

            match handle.get_property_value(GEN_ARRAY_HANDLE) {
                Ok(Property::Array(a)) => {
                    assert_eq!(a.to_string(), "[5.4, 5.4, 5.4, 5.4, 5.4]".to_string(), 
                        "Array to_string produced unexpected result after type change");
                },
                Ok(i) => return Err(format!("Expected still an ArrayHandle, found {:?}", i)),
                Err(e) => return Err(format!("Expected still an ArrayHandle, failed with {}", e.to_string())),
            };

            handle.log_info("Read after type change succeeded");

            assert_eq!(handle.get_property_value(TO_BE_DELETED), Err(DataStoreReturnCode::DoesNotExist), "TO_BE_DELETED should no longer exist");

            handle.send_internal_msg(2);
            decrease_unfinished_tests(&handle)?;
            handle.log_info("Send Internal Plugin Message...");
        },
        Message::Shutdown => {
            assert_eq!(handle.get_property_value(UNFINISHED_TEST), Ok(Property::Int(0)), "Unfinished Tests was not 0, at least one test did not finish");

            handle.log_info("All Tests completed");
            unsafe { datarace_plugin_api::macros::drop_state_now!(handle) }
        },
        Message::InternalMsg(msg) => {
            if msg == 2 {
                handle.log_info(format!("Internal Message received successful"));

                let our = api::generate_foreign_plugin_id(&handle, "test_plugin").ok_or("Failed to aquire own plugin_id".to_string())?;
                let ptr = Box::into_raw(Box::new(69_usize));
                let res = unsafe { handle.send_plugin_ptr_message(our, ptr.cast(), 42) };
                assert!(res.is_ok(), "Failed to send pointer to ourself");

                decrease_unfinished_tests(&handle)?;
                handle.log_info("Send Plugin Ptr Message...");
            }

        },
        Message::PluginMessagePtr { origin, ptr, reason } => {
            let our = api::generate_foreign_plugin_id(&handle, "test_plugin").ok_or("Failed to aquire own plugin_id".to_string())?;
            if origin == our && reason == 42 {
                handle.log_info("Plugin Ptr Message Received");

                let ptr: *mut usize = ptr.cast();
                let v = unsafe { Box::from_raw(ptr) };
                assert_eq!(v.as_ref(), &69, "Unexpected value after dereferencing the pointer");

                handle.log_info("Process Ptr Message successfully");

                handle.trigger_event(EVENT_HANLDE);
                decrease_unfinished_tests(&handle)?;
                handle.log_info("Triggered Event successfully...");
            } else {
                let _ = (origin, ptr, reason); // Technically a memory leak, but who cares
            }
        },
        Message::EventTriggered(ev) => {
            if ev == EVENT_HANLDE {
                handle.log_info("Received Event successfully...");

                handle.unsubscribe_event(EVENT_HANLDE);
                decrease_unfinished_tests(&handle)?;

                handle.log_info("Started unsubscribe...");
            } else {
                handle.log_error("Unknown Event received OwO");
            }
        },
        Message::EventUnsubscribed(ev) => {
            if ev == EVENT_HANLDE {
                handle.log_info("Unsubscribbed from our event");

                let id = if let Ok(action_handle) = datarace_plugin_api::api::generate_action_handle("test_plugin.test") {
                    match handle.trigger_action(action_handle, Some(vec![Property::Int(3)])) {
                        Ok(id) => id,
                        Err(_) => { return Err("Failed to trigger action".to_string()); },
                    }
                } else {
                    return Err("Failed to generate action handle".to_string());
                };
                
                let state = datarace_plugin_api::macros::get_state!(handle).ok_or("No state :(".to_string())?;
                assert_eq!(state.action_id.swap(id, std::sync::atomic::Ordering::Relaxed), u64::MAX, "Action ID was already set once");

                decrease_unfinished_tests(&handle)?;
                handle.log_info(format!("Triggered Action, id {}", id))

            } else {
                handle.log_error("Unknown Event unsubscribed OwO");
            }
        },
        Message::ActionRecv(action) => {
            let our = api::generate_foreign_plugin_id(&handle, "test_plugin").ok_or("Failed to aquire own plugin_id".to_string())?;

            match action.get_action_code() {
                datarace_plugin_api::macros::generate_action_code!("test") => {
                    if action.get_origin() == our {
                        handle.log_info("Our trigger action received, continuing test");

                        let params = action.get_parameters();
                        assert_eq!(params.len(), 1, "Not the correct number of paramters");
                        assert_eq!(params[0], Property::Int(3), "Not the correct parameter value");

                        let state = datarace_plugin_api::macros::get_state!(handle).ok_or("No state :(".to_string())?;
                        assert_eq!(action.get_action_id(), state.action_id.load(std::sync::atomic::Ordering::Relaxed), "Missmatched action id");

                        handle.log_info("Action Received correctly");

                        handle.action_callback(action, 0, Some(vec![Property::from("Action Callback")]));
                        decrease_unfinished_tests(&handle)?;

                        handle.log_info("Send callback...");
                    } else {
                        // For debugging dashboards
                        handle.log_info(format!("Action test performed, caller {}, id {}, parameters {:?}", 
                            action.get_origin(), action.get_action_id(), action.get_parameters()));

                        handle.action_callback(action, 0, None);
                    }
                },
                datarace_plugin_api::macros::generate_action_code!("count") => {
                    if let Ok(Property::Int(v)) = handle.get_property_value(COUNTER) {
                        let v = v + 1;
                        handle.log_info(format!("Counter increased to {v}"));
                        handle.update_property(COUNTER, Property::Int(v));
                    }
                }
                _ => {
                    handle.action_callback(action, 404, None);
                }
            }
            
        },
        Message::ActionCallbackRecv(callback) => {
            let our = api::generate_foreign_plugin_id(&handle, "test_plugin").ok_or("Failed to aquire own plugin_id".to_string())?;
            if callback.get_origin() == our {
                handle.log_info("Received our action callback");

                let params = callback.get_parameters();
                assert_eq!(params.len(), 1, "Not the correct number of paramters");
                assert_eq!(params[0], Property::from("Action Callback"), "Not the correct parameter value");

                let state = datarace_plugin_api::macros::get_state!(handle).ok_or("No state :(".to_string())?;
                assert_eq!(callback.get_action_id(), state.action_id.load(std::sync::atomic::Ordering::Relaxed), "Missmatched action id");

                assert_eq!(callback.get_return_code(), 0, "Unexpected Return Code");

                handle.log_info("Action Callback Processed correctly");

                handle.trigger_event(EVENT_HANLDE);


                decrease_unfinished_tests(&handle)?;
                handle.log_info("Triggered further tests, that should not happen");
            } else {
                // For debugging dashboards
                handle.log_info(format!("Received Callback for Action of id {}: Retrun code {}, parameters {:?}", 
                    callback.get_action_id(), callback.get_return_code(), callback.get_parameters()));
            }
        },


        Message::Unknown => {
            // Fallback, for when the plugin is used with a newer version of libdatarace with more
            // message types
            handle.log_error("Unknown Message Received!");
        }
    }

    Ok(())
}

fn decrease_unfinished_tests(handle: &PluginHandle) -> Result<(), String> {
    if let Ok(Property::Int(v)) = handle.get_property_value(UNFINISHED_TEST) {
        if handle.update_property(UNFINISHED_TEST, Property::Int(v - 1)).is_ok() {
            return Ok(());
        }
    }

    Err("Updating Unfinished Tests failed!".to_string())
}
