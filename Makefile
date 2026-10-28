.PHONY: all build test check clean fmt

all: check test build

build:
	stellar contract build

test:
	cargo test

check:
	cargo check

clean:
	cargo clean

fmt:
	cargo fmt --all

