// This file stores generating proc macros
// They are different as they generate variables and functions at the same time

use proc_macro::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::{parse::Parse, parse_macro_input, punctuated::Punctuated, spanned::Spanned, Expr, Ident, LitStr, Token};

struct PropInitor {
    func_name: Ident,
    plugin_name: LitStr,
    tuples: Punctuated<PropInitTupple, Token![,]>,
}

impl Parse for PropInitor {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let func_name = input.parse()?;
        input.parse::<Token![,]>()?;
        let plugin_name = input.parse()?;
        input.parse::<Token![,]>()?;
        let tuples = input.parse_terminated(PropInitTupple::parse, Token![,])?;

        Ok(PropInitor { func_name, plugin_name, tuples })
    }
}

struct PropInitTupple {
    var_name: Ident,
    prop_name: LitStr,
    init_value: Expr
}

// Bracket: []

impl Parse for PropInitTupple {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content: Expr = input.parse()?;
        let (t_span, mut tuple) = if let Expr::Tuple(t) = content {
            (t.span(),t.elems.into_iter())
        } else {
            return Err(syn::Error::new(content.span(), "Expected Tuple"));
        };
        
        let var_name = match tuple.next() {
            Some(syn::Expr::Path(path)) => {
                if let Some(ident) = path.path.get_ident() {
                    ident.clone()
                } else {
                    return Err(syn::Error::new(path.span(), "Needs to be a simple Variable name"))
                }
            },
            Some(ex) => return Err(syn::Error::new(ex.span(), "Expected const varible name"))            ,
            None => return Err(syn::Error::new(t_span.span(), "Missing const varibale name"))
        };
        
        let prop_name = match tuple.next() {
            Some(Expr::Lit(lit)) => {
                match lit.lit {
                    syn::Lit::Str(strlit) => strlit,
                    _ => return Err(syn::Error::new(lit.span(), "Expected string literal"))
                }
            },
            Some(ex) => return Err(syn::Error::new(ex.span(), "Expected string literal"))            ,
            None => return Err(syn::Error::new(t_span.span(), "Missing string literal"))
        };

        let init_value = tuple.next().ok_or(syn::Error::new(t_span.span(), "Missing init value (literal or lit array)"))?;

        Ok(PropInitTupple {
            var_name,
            prop_name,
            init_value
        })
    }
}

pub(crate) fn property_initor(input: TokenStream) -> TokenStream {
    let PropInitor { func_name, plugin_name, tuples } = parse_macro_input!(input as PropInitor);

    let plugin_name = plugin_name.value();

    let mut declare = vec![];
    let mut inits = vec![];

    for item in tuples {
        let PropInitTupple {
            var_name,
            prop_name,
            init_value
        } = item;

        
        
        let full_prop_name = LitStr::new(format!("{plugin_name}.{}", prop_name.value()).as_str(), prop_name.span());

        declare.push(
            quote!{
                pub const #var_name: datarace_plugin_api::wrappers::PropertyHandle = datarace_plugin_api::macros::generate_property_handle!(#full_prop_name);
            }
        );

        inits.push(
            match init_value {
                Expr::Lit(lit) => {
                    let source = match lit.lit {
                        syn::Lit::Str(litstr) => quote!{ from(#litstr) },
                        syn::Lit::Int(litint) => quote!{ Int(#litint as i64) },
                        syn::Lit::Bool(litbool) => quote!{ Bool(#litbool) },
                        syn::Lit::Float(litfloat) => quote!{ Float(#litfloat as f64) },
                        _ => return quote_spanned!{
                            lit.span() => compile_error!("Unsupported Literal type")
                        }.into_token_stream().into()
                    };

                    quote!{
                        handle.create_property(#prop_name, #var_name, datarace_plugin_api::wrappers::Property::#source)
                            .to_result().map_err(|e| e.to_string())?;
                    }
                },
                Expr::Call(call) => {
                    quote!{
                        handle.create_property(#prop_name, #var_name, datarace_plugin_api::wrappers::Property::from(#call))
                            .to_result().map_err(|e| e.to_string())?;
                    }
                },
                Expr::MethodCall(call) => {
                    quote!{
                        handle.create_property(#prop_name, #var_name, datarace_plugin_api::wrappers::Property::from(#call))
                            .to_result().map_err(|e| e.to_string())?;
                    }
                },
                Expr::Path(p) => {
                    if let Some(p) = p.path.get_ident() {
                        if &Ident::new("None", p.span()) == p {
                            quote!{
                                handle.create_property(#prop_name, #var_name, datarace_plugin_api::wrappers::Property::None)
                                    .to_result().map_err(|e| e.to_string())?;
                            }
                        } else {
                            // For handling consts
                            quote!{
                                handle.create_property(#prop_name, #var_name, datarace_plugin_api::wrappers::Property::from(#p))
                                    .to_result().map_err(|e| e.to_string())?;
                            }
                        }
                    } else {
                        return quote_spanned!{
                            p.span() => compile_error!("Expected init value (Has to be literal or literal array)")
                        }.into_token_stream().into();
                    }
                },

                Expr::Array(arr) => {
                //     let mut iter = arr.elems.iter();
                    return quote_spanned!{
                        arr.span() => compile_error!("Currently arrays can only be instantiated via Repeat Pattern, example: [\"test\", 9]")
                    }.into_token_stream().into();
                },
                Expr::Repeat(arr) => {
                    let source = match *arr.expr {
                        Expr::Lit(lit) => {
                            match lit.lit {
                                syn::Lit::Str(litstr) => quote!{ from(#litstr) },
                                syn::Lit::Int(litint) => quote!{ Int(#litint as i64) },
                                syn::Lit::Bool(litbool) => quote!{ Bool(#litbool) },
                                syn::Lit::Float(litfloat) => quote!{ Float(#litfloat as f64) },
                                _ => return quote_spanned!{
                                    lit.span() => compile_error!("Unsupported Literal type")
                                }.into_token_stream().into()
                            }
                        },
                        Expr::Call(call) => {
                            quote!{
                                from(#call)
                            }
                        },
                        _ => {
                            return quote_spanned!{
                                arr.expr.span() => compile_error!("Unsupported Type")
                            }.into_token_stream().into();
                        }
                    };
                    
                    let len = arr.len;

                    quote! {
                        let arr_handle = datarace_plugin_api::wrappers::ArrayHandle::new(&handle, datarace_plugin_api::wrappers::Property::#source, #len)
                            .ok_or("Failed to create Array".to_string())?;
                        handle.create_property(#prop_name, #var_name, datarace_plugin_api::wrappers::Property::from(arr_handle))
                            .to_result().map_err(|e| e.to_string())?;
                    }
                },
                _ => return quote_spanned!{
                    init_value.span() => compile_error!("Expected init value (Has to be literal or literal array)")
                }.into_token_stream().into()
            }
        );
    }

    

    

    quote! {
        #(#declare)*

        pub fn #func_name(handle: &datarace_plugin_api::wrappers::PluginHandle) -> Result<(), String> {
            #(#inits)*

            Ok(())
        }
    }.into_token_stream().into()
}
