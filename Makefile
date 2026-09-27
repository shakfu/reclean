
.PHONY: all build release test check clippy fmt lint doc clean \
		install uninstall publish publish-dry

all: build

build:
	@cargo build

release:
	@cargo build --release

test:
	@cargo test

check:
	@cargo check

clippy:
	@cargo clippy -- -W clippy::all

fmt:
	@cargo fmt

lint: fmt clippy

doc:
	@cargo doc --no-deps --open

clean:
	@rm -rf target

install: release
	@cp target/release/reclean ~/.local/bin/
	@echo "reclean installed"

uninstall:
	@rm -f /usr/local/bin/reclean
	@echo "reclean uninstalled"

publish-dry:
	@cargo publish --dry-run

publish:
	@cargo publish
