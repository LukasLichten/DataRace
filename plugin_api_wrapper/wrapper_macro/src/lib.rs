use proc_macro::TokenStream;
use quote::{quote, ToTokens};

/// Generates the init function required for your plugin <br>
/// Pass in the name of your function that will handle the startup
#[proc_macro]
pub fn init_fn(input: TokenStream) -> TokenStream {

    let func_name = quote::format_ident!("{}", input.to_string());
    
    quote! {
#[no_mangle]
pub extern "C" fn init(handle: *mut datarace_plugin_api_wrapper::reexport::PluginHandle) -> std::os::raw::c_int {
    #func_name(handle);

    0
}
    }.into_token_stream().into()
}
