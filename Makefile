.phony: all build plugin-api test-plugin clean test final help 

all: build test-plugin
	# Test Plugin temporary build also during make
	LD_LIBRARY_PATH=./target/release/ ./target/release/datarace

build: plugin-api
	cargo build --release

plugin-api:
	cd plugin_api_lib && cargo build --release

test-plugin:
	mkdir -p plugins
	cd sample_plugin && cargo build --release
	cp target/release/libsample_plugin.so plugins/

clean:
	cargo clean
	rm -rf ./plugins

test: 
	# cargo test -p sample_plugin
	echo "TODO"
	

final: clean plugin-api
	rm -fr target/final
	mkdir target/final
	echo "TODO"

help:
	@echo "Makefile for build DataRace"
	@echo "make:             Runs 'make build' and then runs it"
	@echo "make build:       Builds Plugin-API and Executable (release mode)"
	@echo "make plugin-api:  Only builds the plugin-api (release mode)"
	@echo "make test-plugin: Builds the sample plugin"
	@echo "make clean:       Runs cargo clean and deletes the PluginAPI.so (does not delete plugins/)"
	@echo "make test:        TODO Runs tests on plugin api"
	@echo "make final:       TODO Rebuilds and packages the executable for release"
	@echo "make help:        Prints this info"
