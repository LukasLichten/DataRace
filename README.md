
## DataRace
Extendable blazingly fast * multiplattform Realtime Data processing and visualization Engine.  
Perfect for Simracing, Flightsim and Streaming.  
  
Written in Rust with a C ABI plugin api to allow you to write plugins in any language,
or talk to the Socket.io api instead.  
  
**And hopefully not so fast to "Data Race" itself*

## Features
**Unusable**, nothing is implemented yet  
  
*TODO*:  
- Proper Project structure to allow Plugins to be build
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

## Build Instructions
To build the plugin api and the executable run
```
make
```

Also check `make help` for further things  
  
*Small advice: use `ldd` in case having issues running the exec due to missing shared lib*
