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
  
In Progess:  
- Implement socket.io server
- Build dashboard rendering
- Script Engine
  
*Far future*:
- cmd/env args, config files
- Build native game reader
- Build dashboard editor
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
While you can build within this workspace (at which point the lib is provided),
however it is better to have `libdatarace` installed.  
As there are no *public* packages/scripts available, you will need to make your own/do it by hand.  
Make sure `libdatarace.so` is placed in `/usr/lib` and `libdatarace.h` is placed in `/usr/include`.
  
You can develope a Plugin in any programming language that can compile into a dynamic library (dll or so),
that can expose a C-ABI, using the headers `libdatarace.h` generated by this project to interact with the library.  
The Plugin requires the following functions to be loaded:  
```
struct PluginDescription get_plugin_description();
void free_string(char *ptr);
int init(struct PluginHandle *handle);
int update(struct PluginHandle *handle, struct Message msg);
```
  
For rust you can use the crate `datarace_plugin_api` (however, like for building the project, `clang` is required,
and installing `libdatarace` is recommended/required).  
To implement these required functions you should use these macros (further information can be found in the `sample_plugin`):  
```
use datarace_plugin_api::macros::{free_string_fn, plugin_descriptor_fn, plugin_init, plugin_update};
```
Add the api crate add this to your dependencies:  
```
datarace_plugin_api = { git = "https://github.com/LukasLichten/DataRace.git", branch = "master" }
```

#### Dealing with `ProcMacro not expanded` lint
This is a false positive, as the programm will still compile.  
The issue is that `plugin_api_macro` crate depends on the `libdatarace` during compiletime (to generate PropertyHandles, etc).
This is fine, but when your LSP tries to run the Macro functions it is not able run them due to failing to link.  

The solution is to install `libdatarace` on your distro, compiling will still use the library in target first,
before falling back to the OS library.  

### Project Build Instrutions
#### Linux:
To build the plugin api and the executable run
```
make build
make
```

Build the sample_plugin via  
```
make test-plugin
```

Also check `make help` for further options  

#### Windows:
Use this powershell script:
```
.\wmake.ps1
```

### Project Structure
- `launcher`: Houses the executable, which only serves as a launcher.
- `lib`: Main Logic. Both serves to load the plugins (provide datastorage, websocket and server), but also as API for them to link to.
- `plugin_api_sys`: Serves to expose the (raw) function of the API while dealing with the linking.
- `plugin_api_macro`: Proc-Macro Crate, available through `plugin_api::macros`
- `plugin_api`: Provides a wrapper around the sys raw functions. Perfect for implementing plugins.
- `sample_plugin`: An example plugin in rust using the wrapper
  
Rust typically leaves libraries as code that is compiled into the final binary,
so this way of compiling a central api library into C ABI, and then having to use a regular rust crate to link it back (and another to provide a smooth interaction) is weird.  
But this is all done for modularity, because having to for every new plugin installed add code and recompile just is unacceptable.

### Licensing
This project is Licensed under GPLv3.  
Expection is the `sample_plugin`, which can be licensed under GPLv3 or MIT (to avoid restricting plugin developers into a specfic license just because they starter off with the example).  
