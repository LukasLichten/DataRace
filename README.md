## DataRace
Extendable blazingly fast * multiplattform Realtime Data processing and visualization Engine.  
Perfect for Simracing, Flightsim and Streaming.  
  
Written in Rust with a C ABI plugin api to allow you to write plugins in any language,
or talk to the Socket.io api instead.  
  
**And hopefully not so fast to "Data Race" itself*

## Features
**Unusable**, nothing is implemented yet  
  
*TODO*:  
- Implement basic API handles for data and messaging
- Build wrapper
- Implement Sample Plugin
- Insure Windows Compatibility

*Far future*:
- Implement socket.io server
- Build native game reader
- Build dashboard rendering and editor
- Implementing the option to export telemetry logs
- World domination
- Stabilize API lol

## Building
### Build Requirements
As `plugin_api_sys` makes use of bindgen for build, which requires clang.  
More info found here: (rust-bindgen/requirements)[https://rust-lang.github.io/rust-bindgen/requirements.html]  

### Plugin Build Instructions
*TODO*

### Project Build Instrutions
To build the plugin api and the executable run
```
make
```

Also check `make help` for further things  

### Project Structure
- `main`: Houses the executable, which only serves as a launcher.
- `plugin_api_lib`: Main Logic. Both serves to load the plugins, but also as API for them to link to.
- `plugin_api_sys`: Serves to expose the (raw) function of the API while dealing with the linking.
- `plugin_api_wrapper`: Provides a wrapper around the sys raw functions. Perfect for implementing plugins.
  
Rust typically leaves libraries as code that is compiled into the final binary,
so this way of compiling a central api library into C ABI, and then having to use a regular rust crate to link it back (and another to provide a smooth interaction) is weird.
But this is all done for modularity, because having to for every new plugin installed add code and recompile just is unacceptable.
