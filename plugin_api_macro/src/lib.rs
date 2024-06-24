use proc_macro::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::{parse::{Parse, ParseStream}, parse_macro_input, Ident, LitInt, LitStr, Token};

mod attr;

/// Add this the function you want to handle the plugin init.  
/// This function requires to take the PluginHandle as parameter,
/// return type has to be Result<PluginState, String> or Result<(), String>.  
///   
/// Result<PluginState,String> means the state will be saved in the handle after return Ok.
/// This is simpler then calling save_state!(handle), but does not work if you want to start a
/// worker thread in init, as the save would occure after the thread is started, therefore a race
/// condition.  
/// Result<(),String> is ideal for this, but expects from you to save manually
///   
/// String is not a requirement, but the Err type must implement ToString (this may change in the
/// future, requiering a specific type).  
/// If you return Err the plugin is considered failed and will be shut down.  
///   
/// The function can not be async or ffi-abi, but can be unsafe. Visibility keywords (like pub) will be
/// ignored, the function will be internal to the generated `extern "C" fn init`
#[proc_macro_attribute]
pub fn plugin_init(attr: TokenStream, item: TokenStream) -> TokenStream {
    attr::plugin_init(attr, item)
}

/// Add this the function you want to handle the plugin update.  
/// This function requires to take the PluginHandle and Message as parameters,
/// return type has to be Result<(), String>.  
///   
/// String is not a requirement, but the Err type must implement ToString (this may change in the
/// future, requiering a specific type).  
/// If you return Err the plugin is considered failed and will be shut down.  
///   
/// The function can not be async or ffi-abi, but can be unsafe. Visibility keywords (like pub) will be
/// ignored, the function will be internal to the generated `extern "C" fn update`
#[proc_macro_attribute]
pub fn plugin_update(attr: TokenStream, item: TokenStream) -> TokenStream {
    attr::plugin_update(attr, item)
}

struct DescriptorTokens {
    plugin_name: LitStr,
    version_major: LitInt,
    version_minor: LitInt,
    version_patch: LitInt
}

impl Parse for DescriptorTokens {
    fn parse(input: ParseStream) -> syn::parse::Result<Self> {
        let plugin_name: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let version_major: LitInt = input.parse()?;
        input.parse::<Token![,]>()?;
        let version_minor: LitInt = input.parse()?;
        input.parse::<Token![,]>()?;
        let version_patch: LitInt = input.parse()?;

        Ok(DescriptorTokens {
            plugin_name,
            version_major,
            version_minor,
            version_patch
        })
    }
}

/// Generates the get_plugin_description function REQUIERED for your plugin <br>
/// Pass in the name of your plugin, version major, version minor, version patch
/// You have to pass in literals
///
/// Name of your plugin is case sensitive on log and other user facing displays, however for
/// generation fo the plugin id (like in the PropertyHandle) it will be treated case insensitive
#[proc_macro]
pub fn plugin_descriptor_fn(input: TokenStream) -> TokenStream {
    // Wouldn't it be great to parse functions/const retrievals?
    // Well, too bad, this is technically possible, but proc macros make it borderline impossible
    // If YOU really want this, then make a pull request, this is too much bullshit for me.
    let DescriptorTokens {
        plugin_name,
        version_major,
        version_minor,
        version_patch
    } = parse_macro_input!(input as DescriptorTokens);

    let api_version = unsafe {
        datarace_plugin_api_sys::compiletime_get_api_version()
    };

    let plugin_name_str = plugin_name.value();
    let id = unsafe {
        let ptr = std::ffi::CString::new(plugin_name_str).expect("plugin name can not be converted into CString").into_raw();
        let res = datarace_plugin_api_sys::compiletime_get_plugin_name_hash(ptr);

        drop(std::ffi::CString::from_raw(ptr));

        if !res.valid {
            return quote_spanned! {
                plugin_name.span() => compile_error!("invalid plugin name")
            }.into_token_stream().into();
        }

        res.id
    };

    quote! {
#[no_mangle]
pub extern "C" fn get_plugin_description() -> datarace_plugin_api::reexport::PluginDescription {
    datarace_plugin_api::reexport::PluginDescription {
        id: #id,
        name: std::ffi::CString::new(#plugin_name).expect("string is string").into_raw(),
        version: [#version_major, #version_minor, #version_patch],
        api_version: #api_version,
    }
}
    }.into_token_stream().into()
}

/// Generates the free_string function REQUIRED for your plugin <br>
/// Purpose of this function is to deallocate strings allocated by this plugin <br>
/// This standard definition should be sufficient for most use-cases
#[proc_macro]
pub fn free_string_fn(_input: TokenStream) -> TokenStream {
    quote! {
        
#[no_mangle]
pub extern "C" fn free_string(ptr: *mut std::os::raw::c_char) {
    unsafe {
        drop(std::ffi::CString::from_raw(ptr));
    }

}
    }.into_token_stream().into() 
}


/// Generates a property handle at compiletime
/// It will insert a PropertyHandle in this place
///
/// This is perfect for propertys with static values, as this cuts the need of sending a cstring
/// and the api hashing it during runtime.
/// But if you need dynamics, the function by the same name is a better choice (and you can store
/// the results of that one too)
///
/// Property names are not case sensitive, have to contain at least one dot, with the first dot
/// deliminating between plugin and property (but the property part can contain further dots).
/// You can not have any leading or trailing dots
#[proc_macro]
pub fn generate_property_handle(input: TokenStream) -> TokenStream {
    let name = parse_macro_input!(input as LitStr);

    let name_val = name.value();
    let handle = unsafe {
        let ptr = std::ffi::CString::new(name_val).expect("name can not be converted into CString").into_raw();
        let res = datarace_plugin_api_sys::generate_property_handle(ptr);

        drop(std::ffi::CString::from_raw(ptr));

        if res.code != datarace_plugin_api_sys::DataStoreReturnCode_Ok {
            return quote_spanned! {
                name.span() => compile_error!("invalid name")
            }.into_token_stream().into();
        }

        res.value
    };

    let id = handle.plugin;
    let prop = handle.property;

    quote! {
        unsafe {
            datarace_plugin_api::wrappers::PropertyHandle::from_values(#id, #prop)
        }
    }.into_token_stream().into()
}

struct StateSaveTokens {
    handle_name: Ident,
    state: Ident
}

impl Parse for StateSaveTokens {
    fn parse(input: ParseStream) -> syn::parse::Result<Self> {
        let handle_name: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let state: Ident = input.parse()?;

        Ok(StateSaveTokens {
            handle_name,
            state
        })
    }
}

/// This stores the state (specifically the pointer for the state) into the plugin handle.
/// This requires setting a up in the root of your Plugin this:
/// ```
/// pub type PluginState = YourState
/// ```
/// Then you can pass an instance of `YourState` into this function.  
///
/// This is unsafe as there can be race conditions with get_state, so this is best
/// used during init, prior to spinning up any other task.
/// 
/// The previous state is NOT deallocated, so you either call drop_state_now! first,
/// or use handle.get_state_ptr() to retrieve the pointer (which you then deallocate after calling this
/// function. Keep in mind, that deallocating is even more unsafe as just using this function, read
/// more in drop_state_now!).
/// Obviously you don't need to deallocate anything if you had never assigned a state before
#[proc_macro]
pub fn save_state_now(input: TokenStream) -> TokenStream {
    let StateSaveTokens {
        handle_name,
        state
    } = parse_macro_input!(input as StateSaveTokens);


    quote! {
    {
        let t: crate::PluginState = #state;
        let ptr = Box::into_raw(Box::new(t));
        
        let ptr = ptr.cast::<std::os::raw::c_void>();

        #handle_name.store_state_ptr_now(ptr);
    }
    }.into_token_stream().into()

}

/// This retrieves a Reference to the PluginState from the PluginHandle (set by save_state_now! or init).  
/// This requires setting a up in the root of your Plugin this:
/// ```
/// pub type PluginState = YourState
/// ```
///
/// To use this pass in the PluginHandle
///
/// This function is save, however improper use can lead to unsafe memory access:
/// - Calling drop_state_now! (or manually deallocating the pointer) while holding the reference returned by this
/// - Using handle.store_state_ptr_now() with a pointer to an Object that is not of type PluginState
/// - Setting a new State through save_state_now! while holding this reference will not update it
#[proc_macro]
pub fn get_state(input: TokenStream) -> TokenStream {
    let handle_name = parse_macro_input!(input as Ident);


    quote! {
        unsafe {
            let ptr = #handle_name.get_state_ptr(); 
            ptr.cast::<crate::PluginState>().as_ref()
        } 
    }.into_token_stream().into()
}

/// This drops the current State object stored in the PluginHandle
/// This requires setting a up in the root of your Plugin this:
/// ```
/// pub type PluginState = YourState
/// ```
///
/// To use this pass in the PluginHandle
///
/// This function is incredibly unsafe, as (in a multi threaded enviorment, or having handed off
/// closures) someone could still be holding a Reference to this state optained through get_state!,
/// which would then point into freed memory.
/// So this function should only be called during shutdown, after all potentially conflicting
/// states have been shut down.
/// Additional unsafety is that the pointer in the handle could have been set via handle.store_state_ptr_now() to
/// a type that is not PluginState.
///
/// This function minds the case of the ptr being null, and also sets the pointer to null
#[proc_macro]
pub fn drop_state_now(input: TokenStream) -> TokenStream {
    let handle_name = parse_macro_input!(input as Ident);

    quote! {
        {
            let ptr = #handle_name.get_state_ptr().cast::<crate::PluginState>();
            
            if !ptr.is_null() {
                #handle_name.store_state_ptr_now(std::ptr::null_mut());
                
                drop(Box::from_raw(ptr));
            }
        }
    }.into_token_stream().into()
}
