.phony: all build build-exec plugin-api test-plugin clean test help release

define exec
	LD_LIBRARY_PATH=./target/debug/ ./target/debug/launch_datarace
endef

all: plugin-api
	# only building the plugin-api, compile the plugin seperatly
	$(exec)

build: plugin-api build-exec

build-exec: 
	cargo build

run:
	$(exec)

plugin-api:
	cd lib && cargo build

test-plugin:
	mkdir -p plugins
	cd test_plugin && cargo build
	cp target/debug/libtest_plugin.so plugins/

sample-plugin:
	mkdir -p plugins
	cd sample_plugin && cargo build
	cp target/debug/libsample_plugin.so plugins/

clean:
	cargo clean
	rm -rf ./plugins

test: 
	cargo test -p datarace
	cargo test -p datarace_plugin_api_sys

release:
	cd lib && cargo build --release
	cargo build --release

help:
	@echo "Makefile for build DataRace"
	@echo "make:               Runs 'make plugin-api' and then runs (use build-exec prior to this)"
	@echo "make build:         Builds Plugin-API and Executable"
	@echo "make plugin-api:    Only builds the plugin-api"
	@echo "make build-exec:    Only builds the executable"
	@echo "make run:           Runs it"
	@echo "make test-plugin:   Builds the tester plugin"
	@echo "make sample-plugin: Builds the sample plugin"
	@echo "make release:       Does Optimized Release Build"
	@echo "make clean:         Runs cargo clean and deletes the PluginAPI.so (does not delete plugins/)"
	@echo "make test:          Runs static tests on the lib and sys"
	@echo "make help:          Prints this info"
