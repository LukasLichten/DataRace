// use bindgen;
// use std::{env, path::PathBuf};
use std::path::PathBuf;

fn main() {

    let bin = PathBuf::from("../bin").canonicalize().expect("Root of all evil!");

    println!("cargo:rustc-link-search={}",bin.to_str().unwrap());
    println!("cargo:rustc-link-lib=dylib=datarace_plugin_api");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    // println!("cargo:rerun-if-changed=api_sys/libdatarace_plugin_api.h");
    //
    // // The bindgen::Builder is the main entry point
    // // to bindgen, and lets you build up options for
    // // the resulting bindings.
    // let bindings = bindgen::Builder::default()
    //     // The input header we would like to generate
    //     // bindings for.
    //     .header("libdatarace_plugin_api.h")
    //     // Tell cargo to invalidate the built crate whenever any of the
    //     // included header files changed.
    //     .parse_callbacks(Box::new(bindgen::CargoCallbacks))
    //     // Finish the builder and generate the bindings.
    //     .generate()
    //     // Unwrap the Result and panic on failure.
    //     .expect("Unable to generate bindings");
    //
    // // Write the bindings to the $OUT_DIR/bindings.rs file.
    // let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    // bindings
    //     .write_to_file(out_path.join("bindings.rs"))
    //     .expect("Couldn't write bindings!");
}
