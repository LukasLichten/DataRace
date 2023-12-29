use bindgen;
use std::{env, path::PathBuf};

// Env Variables
const ENV_LIB_PATH: &str = "DATARACE_PLUGIN_API_LIB_PATH";
const ENV_HEADER_FILE: &str = "DATARACE_PLUGIN_API_HEADER_FILE";

pub fn main() {
    // Linking
    let lib = find_and_bind_lib(true);
    
    // Header File Getting
    let h_path = if let Ok(path) = env::var(ENV_HEADER_FILE) {
        let path = PathBuf::from(path);

        if !path.is_file() {
            panic!("Could not find a File at {}. Either specify a path to a valid header file, or unset '{}' to let it generate automatically", path.to_str().unwrap(), ENV_HEADER_FILE);
        }

        if let Ok(path) = path.canonicalize() {
            path
        } else {
            panic!("Unable to canonicalize the path for the header file. It is advisable to provide absolute paths");
        }
    } else {
        let target = lib.parent().unwrap().join(lib.file_stem().unwrap().to_str().unwrap().to_string() + ".h");
        
        if !target.is_file() {
            panic!("No header file present in output!");
        }
        target
    };
    

    println!("cargo:rerun-if-changed={}",h_path.to_str().unwrap());

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header(h_path.to_str().unwrap())
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings for datarace_plugin_api_lib, did you generate C bindings?");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

fn find_and_bind_lib(mut try_env: bool) -> PathBuf {
    let env_var = if try_env {
        env::var(ENV_LIB_PATH).ok()
    } else {
        None
    };

    let bin = if let Some(path) = env_var {
        // Env overright was set
        let mut path = PathBuf::from(path);
        
        if path.is_file() {
            path.pop(); // We can't let you override the library name
        }

        if let Ok(path) = path.canonicalize() {
            path
        } else {
            println!("cargo:warning=Unable to process path provided in '{}', attempting default...", ENV_LIB_PATH);
            return find_and_bind_lib(false);
        }
    } else {
        try_env = false;
        PathBuf::from(env::var("OUT_DIR").expect("Rust build system is no longer setting OUT_DIR? Is it snowing in hell?")).join("../../../").
            canonicalize().expect("Failed to get to the binary output folder in target... something is a bit fishy...")
    };


    // Testing if Library is present
    let lib = if cfg!(target_os = "linux") {
        let test = bin.join("libdatarace_plugin_api.so");
        if !test.exists() {
            if try_env {
                // We failed to find the library where specified, but we can still retry default
                println!("cargo:warning=Unable find library at {}, attempting at default...", bin.to_str().unwrap());
                return find_and_bind_lib(false);
            }

            panic!("Unable to find libdatarace_plugin_api.so within output directory! Make sure to build plugin_api_lib first (and in the same release mode)!");
        } else {
            // Rerun if the library has been updated
            // Also 
            println!("cargo:rerun-if-changed={}",test.to_str().unwrap());
            test
        }
    } else if cfg!(target_os = "windows") {
        let test = bin.join("datarace_plugin_api.dll.lib");
        if !test.exists() {
            if try_env {
                // We failed to find the library where specified, but we can still retry default
                println!("cargo:warning=Unable find library at {}, attempting at default...", bin.to_str().unwrap());
                return find_and_bind_lib(false);
            }

            panic!("Unable to find datarace_plugin_api.dll.lib within output directory! Make sure to build plugin_api_lib first (and in the same release mode)!");
        } else {
            // Rerun if the library has been updated
            // Also 
            println!("cargo:rerun-if-changed={}",test.to_str().unwrap());
            bin.join("datarace_plugin_api.dll")
        }
    } else {
        println!("cargo:warning=Unable to verify if Library is present... Unknown Plattform");
        bin.join("datarace_plugin_api.dylib")
    };

    println!("cargo:rustc-link-search={}",bin.to_str().unwrap());
    if cfg!(target_os = "windows") {
        // Windows linker wants the .lib, rust builds *.dll, *.dll.lib, etc.
        // But if we just give it the normal name it will look for *.lib, so this is the work around for it
        println!("cargo:rustc-link-lib=dylib=datarace_plugin_api.dll");
    } else {
        println!("cargo:rustc-link-lib=dylib=datarace_plugin_api");
    }

    lib
}
