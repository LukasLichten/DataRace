## DataRace
Extendable blazingly fast * multiplattform Realtime Data processing and visualization Engine.  
Perfect for Simracing, Flightsim and Streaming.  
  
Written in Rust with a C ABI plugin api to allow you to write plugins in any language,
or talk to the Socket.io api instead.  
  
**And hopefully not so fast to "Data Race" itself*

## Features
**Unusable**, basically nothing is implemented yet:  
- Linux and Windows Compat
- Loading of Plugins out of the plugins folder
- Implement basic API handles for data and messaging
- Flesh out wrapper and sample plugin

*Far future*:
- cmd/env args, config files
- Implement socket.io server
- Build native game reader
- Build dashboard rendering and editor
- Implementing the option to export telemetry logs
- World domination
- Stabilize API lol

Some small features are up for consideration, ready to implement,
but to stop myself from getting bucked by scope creep more then necessary I have instead put them [here](docs/MayImplement.md)

## Building
### Build Requirements
As `plugin_api_sys` makes use of bindgen for build, which requires clang.  
More info found here: [rust-bindgen/requirements](https://rust-lang.github.io/rust-bindgen/requirements.html)  

### Plugin Build Instructions
*TODO*
#### Dealing with `ProcMacro not expanded` lint
This is a false positive, as the programm will still compile.  
This happens due to the wrapper_macro crate (which you access through `datarace_plugin_api_wrapper::macros::*`)
also depending on `plugin_api_sys`, as it accesses the library during compiletime to define values (such as apiversion and name hashes).  
There are two issues:  
First, rust_analyer will make use of Debug mode, we usually compile in Release,
so `libdatarace_plugin_api.so` does not exist (and it can't know that it could just compile it into existence), so `plugin_api_sys` fails to compile,
`wrapper_macro` fails to compile, and we get `no proc macro present for crate`.  
Second, even if you compile the lib in debug it will now fail with a new error. This one is due to the `wrapper_macro` being turned into a `.so`,
and when rust_analyzer tries to invoke it the `libdatarace_plugin_api.so` (to which it links) can not be found, as while it is in the same folder,
Linux links only to libraries in very specific places such as `/usr/lib`. A fix would be to place a version of `libdatarace_plugin_api.so` in there.  
  
As such I will stamp this off as a development only problem, once (in the far far future, humaity has colonized the galaxy...) this has a released version,
available through package managers, this should be a none issue for plugin devs, as it would just build & runtime link to the version installed on their system

### Project Build Instrutions
#### Linux:
To build the plugin api and the executable run
```
make
```

Also check `make help` for further things  

#### Windows:
Use this powershell script:
```
.\wmake.ps1
```

### Project Structure
- `main`: Houses the executable, which only serves as a launcher.
- `plugin_api_lib`: Main Logic. Both serves to load the plugins, but also as API for them to link to.
- `plugin_api_sys`: Serves to expose the (raw) function of the API while dealing with the linking.
- `plugin_api_wrapper`: Provides a wrapper around the sys raw functions. Perfect for implementing plugins.
- `sample_plugin`: An example plugin in rust using the wrapper
  
Rust typically leaves libraries as code that is compiled into the final binary,
so this way of compiling a central api library into C ABI, and then having to use a regular rust crate to link it back (and another to provide a smooth interaction) is weird.  
But this is all done for modularity, because having to for every new plugin installed add code and recompile just is unacceptable.
