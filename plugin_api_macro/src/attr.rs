use proc_macro::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::{parse_macro_input, spanned::Spanned, FnArg, GenericArgument, Ident, ItemFn, Path, PathArguments, Signature, Type};

fn is_type_pluginstate(path: &Path) -> Result<(), TokenStream> {
    let mut has_crate = false;
    for item in path.segments.iter() {
        if item.ident == "crate" && !has_crate {
            has_crate = true;
        } else if item.ident == "PluginState" && item.arguments.is_none() {
            return Ok(());
        } else {
            break;
        }
    }

    Err(quote_spanned! {
        path.span() => compile_error!("only permissable types are () or crate::PluginState")
    }.into_token_stream().into())
}

fn is_plugin_handle(arg: Option<&FnArg>, signatur: &Signature) -> Result<Ident, TokenStream> {
    if let Some(FnArg::Typed(handle)) = arg {
        if let Type::Path(res) = *handle.ty.clone() {
            for item in res.path.segments.iter() {
                if item.ident == "reexport" || item.ident == "datarace_plugin_api_sys" {
                    return Err(quote_spanned! {
                        res.path.span() => compile_error!("please use datarace_plugin_api::wrappers::PluginHandle as the argument type.\nDo Not use this macro if you intend a raw implementation.")
                    }.into_token_stream().into());
                }    
            }

            if let Some(seg) = res.path.segments.iter().last() {
                if seg.ident == "PluginHandle" && seg.arguments.is_none() {
                    if let syn::Pat::Ident(name) = &*handle.pat {
                        return Ok(name.ident.clone())
                    }
                }
            }
        }
    } 

    
    Err(quote_spanned! {
        signatur.inputs.span() => compile_error!("function requiers PluginHandle as argument")
    }.into_token_stream().into())
}

fn is_sig_valid(sig: &Signature) -> Result<bool, TokenStream> {
    if let Some(abi) = &sig.abi {
        return Err(quote_spanned! {
            abi.span() => compile_error!("function has to be regular function")
        }.into_token_stream().into());
    }
    if let Some(a) = &sig.asyncness {
        return Err(quote_spanned! {
            a.span() => compile_error!("function can Not be async function")
        }.into_token_stream().into());
    }
    if !sig.generics.params.is_empty() {
        return Err(quote_spanned! {
            sig.generics.span() => compile_error!("function can not have generics")
        }.into_token_stream().into());
    }

    Ok(sig.unsafety.is_some()) // We permit unsafe init and update functions
}

/// Actual implementation of plugin_init
pub(crate) fn plugin_init(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let ItemFn {
        sig,
        vis: _, // TODO perhaps add warning that vis is ignored when set
        block,
        attrs
    } = parse_macro_input!(item as ItemFn);

    let func_name = sig.ident.clone();

    // Checking function signatures for weird modifiers, and generating call
    let func_call = match is_sig_valid(&sig) {
        Ok(true) => quote!{ unsafe { #func_name(han) } },
        Ok(false) => quote!{ #func_name(han) },
        Err(e) => return e
    };


    // Processing and checking arguments for the function
    let mut iter = sig.inputs.iter();
    let _handle_arg_name = match is_plugin_handle(iter.next(), &sig) {
        Ok(arg_name) => arg_name,
        Err(e) => return e
    };
    
    if let Some(res) = iter.next() {
        return quote_spanned! {
            res.span() => compile_error!("only a single parameter permited")
        }.into_token_stream().into();
    }


    // Processing return type, and generating function call based on it
    let (init_handle, _auto_save) = if let syn::ReturnType::Type(_,  res) = sig.output.clone() {
        if let Type::Path(res) = *res {
            let seg = res.path.segments.iter().next().expect("return type needs to be present");
            if seg.ident == "Result" {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    let mut iter = args.args.iter();
                    
                    if let (Some(GenericArgument::Type(target)), Some(GenericArgument::Type(Type::Path(_re)))) = (iter.next(), iter.next()) {
                        // For now no type checking on the Err

                        match target {
                            Type::Path(res) => {
                                if let Err(e) = is_type_pluginstate(&res.path) {
                                    return e;
                                }

                                // Returns the state, so we save it
                                (quote! {
                                    match #func_call {
                                        Ok(value) => {
                                            unsafe {
                                                let han = datarace_plugin_api::wrappers::PluginHandle::new(handle);
                                                datarace_plugin_api::macros::save_state_now!(han, value);
                                            }
                                            Ok(())
                                        },
                                        Err(e) => Err(e)
                                    }
                                }, true)
                            },
                            Type::Tuple(empty) => {
                                if empty.elems.is_empty() {
                                    (quote!{ #func_call }, false)
                                } else {
                                    return quote_spanned! {
                                        target.span() => compile_error!("Only permissable return types are () and crate::PluginState")
                                    }.into_token_stream().into();
                                }
                            },
                            _ => {
                                return quote_spanned! {
                                    target.span() => compile_error!("Only permissable return types are () and crate::PluginState")
                                }.into_token_stream().into();
                            }
                        }
                    } else {
                        return quote_spanned! {
                            seg.arguments.span() => compile_error!("Result requires two types")
                        }.into_token_stream().into();
                    }
                     
                } else {
                    return quote_spanned! {
                        seg.arguments.span() => compile_error!("Result requires two types")
                    }.into_token_stream().into();
                }

            } else {
                return quote_spanned! {
                    sig.output.span() => compile_error!("Return type Result required!")
                }.into_token_stream().into();
            }  
            
        } else {
            return quote_spanned! {
                sig.output.span() => compile_error!("Malformed Return typing")
            }.into_token_stream().into();
        }
    } else {
        return quote_spanned! {
            sig.output.span() => compile_error!("Return type Result required!")
        }.into_token_stream().into();
    };

    // TODO validate that the plugin handle is not missused in the code block in relation to state
    // Ergo: if auto_save on, that the handle is not cloned, moved into closures etc
    // if auto_save off, that every code path to okay has a save, and that there are no
    // clones/moves prior to a save

    // Code generation
    quote! {
#[no_mangle]
pub extern "C" fn init(handle: *mut datarace_plugin_api::reexport::PluginHandle) -> std::os::raw::c_int {
    #(#attrs)*
    #sig #block

    let han = unsafe { datarace_plugin_api::wrappers::PluginHandle::new(handle) };
    let res = std::panic::catch_unwind(|| {
        #init_handle
    });

    match res {
        Ok(Ok(_)) => 0,
        Ok(Err(text)) => {
            let han = unsafe { datarace_plugin_api::wrappers::PluginHandle::new(handle) };
            han.log_error(text.to_string());
            1
        },
        Err(_) => {
            let han = unsafe { datarace_plugin_api::wrappers::PluginHandle::new(handle) };
            han.log_error("Plugin Init Paniced!");
            10
        }
    }
}
    }.into_token_stream().into()
}

fn is_message(arg: Option<&FnArg>, signatur: &Signature) -> Result<Ident, TokenStream> {
    if let Some(FnArg::Typed(handle)) = arg {
        if let Type::Path(res) = *handle.ty.clone() {
            for item in res.path.segments.iter() {
                if item.ident == "reexport" || item.ident == "datarace_plugin_api_sys" {
                    return Err(quote_spanned! {
                        res.path.span() => compile_error!("please use datarace_plugin_api::wrappers::Message as the argument type.\nDo Not use this macro if you intend a raw implementation.")
                    }.into_token_stream().into());
                }    
            }

            if let Some(seg) = res.path.segments.iter().last() {
                if seg.ident == "Message" && seg.arguments.is_none() {
                    if let syn::Pat::Ident(name) = &*handle.pat {
                        return Ok(name.ident.clone())
                    }
                }
            }
        }
    } 

    
    Err(quote_spanned! {
        signatur.inputs.span() => compile_error!("function requiers Message as argument")
    }.into_token_stream().into())
}

/// Actual implementation of plugin_update
pub(crate) fn plugin_update(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let ItemFn {
        sig,
        vis: _, // TODO perhaps add warning that vis is ignored when set
        block,
        attrs
    } = parse_macro_input!(item as ItemFn);

    let func_name = sig.ident.clone();

    // Checking function signatures for weird modifiers, and generating call
    let func_call = match is_sig_valid(&sig) {
        Ok(true) => quote!{ unsafe { #func_name(han, message) } },
        Ok(false) => quote!{ #func_name(han, message) },
        Err(e) => return e
    };


    // Processing and checking arguments for the function
    let mut iter = sig.inputs.iter();
    if let Err(e) = is_plugin_handle(iter.next(), &sig) {
        return e
    }
    if let Err(e) = is_message(iter.next(), &sig) {
        return e;
    }

    
    if let Some(res) = iter.next() {
        return quote_spanned! {
            res.span() => compile_error!("only two parameters permited")
        }.into_token_stream().into();
    }


    // Processing return type, and generating function call based on it
    let update_handle = if let syn::ReturnType::Type(_,  res) = sig.output.clone() {
        if let Type::Path(res) = *res {
            let seg = res.path.segments.iter().next().expect("return type needs to be present");
            if seg.ident == "Result" {
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    let mut iter = args.args.iter();
                    
                    if let (Some(GenericArgument::Type(target)), Some(GenericArgument::Type(Type::Path(_re)))) = (iter.next(), iter.next()) {
                        // For now no type checking on the Err

                        match target {
                            Type::Tuple(empty) => {
                                if empty.elems.is_empty() {
                                    quote!{ #func_call }
                                } else {
                                    return quote_spanned! {
                                        target.span() => compile_error!("Only permissable return types is ()")
                                    }.into_token_stream().into();
                                }
                            },
                            _ => {
                                return quote_spanned! {
                                    target.span() => compile_error!("Only permissable return type is ()")
                                }.into_token_stream().into();
                            }
                        }
                    } else {
                        return quote_spanned! {
                            seg.arguments.span() => compile_error!("Result requires two types")
                        }.into_token_stream().into();
                    }
                     
                } else {
                    return quote_spanned! {
                        seg.arguments.span() => compile_error!("Result requires two types")
                    }.into_token_stream().into();
                }

            } else {
                return quote_spanned! {
                    sig.output.span() => compile_error!("Return type Result required!")
                }.into_token_stream().into();
            }  
            
        } else {
            return quote_spanned! {
                sig.output.span() => compile_error!("Malformed Return typing")
            }.into_token_stream().into();
        }
    } else {
        return quote_spanned! {
            sig.output.span() => compile_error!("Return type Result required!")
        }.into_token_stream().into();
    };

    // Again, TODO, we could analyze to code block and determine if there is a need to generate a
    // deallocate call for the state

    // Code generation
    quote! {
#[no_mangle]
pub extern "C" fn update(handle: *mut datarace_plugin_api::reexport::PluginHandle, msg: datarace_plugin_api::reexport::Message) -> std::os::raw::c_int {
    #(#attrs)*
    #sig #block


    let han = unsafe { datarace_plugin_api::wrappers::PluginHandle::new(handle) };
    let message = datarace_plugin_api::wrappers::Message::from(msg);
    let res = std::panic::catch_unwind(|| {
        #update_handle
    });

    match res {
        Ok(Ok(_)) => 0,
        Ok(Err(text)) => {
            let han = unsafe { datarace_plugin_api::wrappers::PluginHandle::new(handle) };
            han.log_error(text.to_string());
            1
        },
        Err(_) => {
            let han = unsafe { datarace_plugin_api::wrappers::PluginHandle::new(handle) };
            han.log_error("Plugin Update Paniced!");
            10
        }
    }
}
    }.into_token_stream().into()
}
