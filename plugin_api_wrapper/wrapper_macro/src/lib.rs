use proc_macro::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::{parse::{Parse, ParseStream}, parse_macro_input, LitInt, LitStr, Token};

/// Generates the init function REQUIRED for your plugin <br>
/// Pass in the name of your function that will handle the startup<br>
/// This function needs to take a wrapper::PluginHandle as parameter, and return a Result<(),String><br>
/// <br>
/// Return Ok() if everything worked, use Err if not and to log the message.<br>
/// Also, you don't have to use String, any type that implements ToString works (as long as you
/// didn't Box it).<br>
/// <br>
/// If you return Err or panic a none 0 code is returned to DataRace, which will halt the execution
/// of this plugin.
#[proc_macro]
pub fn init_fn(input: TokenStream) -> TokenStream {

    let func_name = quote::format_ident!("{}", input.to_string());
    
    quote! {
#[no_mangle]
pub extern "C" fn init(handle: *mut datarace_plugin_api_wrapper::reexport::PluginHandle) -> std::os::raw::c_int {
    let han = datarace_plugin_api_wrapper::wrappers::PluginHandle::new(handle);
    let res = std::panic::catch_unwind(|| {
        #func_name(han)
    });

    match res {
        Ok(Ok(_)) => 0,
        Ok(Err(text)) => {
            datarace_plugin_api_wrapper::api::log_error(&datarace_plugin_api_wrapper::wrappers::PluginHandle::new(handle), text.to_string());
            1
        },
        Err(_) => {
            datarace_plugin_api_wrapper::api::log_error(&datarace_plugin_api_wrapper::wrappers::PluginHandle::new(handle), "Plugin Init Paniced!");
            10
        }
    }
}
    }.into_token_stream().into()
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
#[proc_macro]
pub fn plugin_descriptor_fn(input: TokenStream) -> TokenStream {
    let DescriptorTokens {
        plugin_name,
        version_major,
        version_minor,
        version_patch
    } = parse_macro_input!(input as DescriptorTokens);

    let api_version = unsafe {
        datarace_plugin_api_sys::compiletime_get_api_version()
    };
    
    // TODO compiletime id generation
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
pub extern "C" fn get_plugin_description() -> datarace_plugin_api_wrapper::reexport::PluginDescription {
    datarace_plugin_api_wrapper::reexport::PluginDescription {
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

/// Generates the update function REQUIRED for your plugin <br>
/// Pass in the name of your function that will handle the update messages<br>
/// This function needs to take a wrapper::PluginHandle and wrapper::Message as parameter, and return a Result<(),String><br>
/// <br>
/// Return Ok() if everything worked, use Err if not and to log the message.<br>
/// Also, you don't have to use String, any type that implements ToString works (as long as you
/// didn't Box it).<br>
/// <br>
/// If you return Err or panic a none 0 code is returned to DataRace, which will halt the execution
/// of this plugin.
#[proc_macro]
pub fn update_fn(input: TokenStream) -> TokenStream {

    let func_name = quote::format_ident!("{}", input.to_string());
    
    quote! {
#[no_mangle]
pub extern "C" fn update(handle: *mut datarace_plugin_api_wrapper::reexport::PluginHandle, msg: datarace_plugin_api_wrapper::reexport::Message) -> std::os::raw::c_int {
    let han = datarace_plugin_api_wrapper::wrappers::PluginHandle::new(handle);
    let message = datarace_plugin_api_wrapper::wrappers::Message::from(msg);
    let res = std::panic::catch_unwind(|| {
        #func_name(han, message)
    });

    match res {
        Ok(Ok(_)) => 0,
        Ok(Err(text)) => {
            datarace_plugin_api_wrapper::api::log_error(&datarace_plugin_api_wrapper::wrappers::PluginHandle::new(handle), text.to_string());
            1
        },
        Err(_) => {
            datarace_plugin_api_wrapper::api::log_error(&datarace_plugin_api_wrapper::wrappers::PluginHandle::new(handle), "Plugin Update Paniced!");
            10
        }
    }
}
    }.into_token_stream().into()
}
