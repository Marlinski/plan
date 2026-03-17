BIN       := plan
INSTALL   := $(HOME)/.local/bin
SKILL_DIR := $(HOME)/.local/share/plan

.PHONY: build release install uninstall test fmt lint clean

## Build debug binary
build:
	cargo build

## Build optimized release binary
release:
	cargo build --release

## Install release binary and SKILL.md locally
install: release
	mkdir -p $(INSTALL) $(SKILL_DIR)
	cp target/release/$(BIN) $(INSTALL)/$(BIN)
	cp SKILL.md $(SKILL_DIR)/SKILL.md
	@echo "Installed $(INSTALL)/$(BIN)"

## Remove installed binary and skill file
uninstall:
	rm -f $(INSTALL)/$(BIN) $(SKILL_DIR)/SKILL.md
	@echo "Uninstalled $(BIN)"

## Run tests
test:
	cargo test

## Format source code
fmt:
	cargo fmt

## Run clippy lints
lint:
	cargo clippy -- -D warnings

## Remove build artifacts
clean:
	cargo clean

## Tag and push a release (usage: make tag VERSION=v0.2.0)
tag:
	@[ -n "$(VERSION)" ] || (echo "Usage: make tag VERSION=v0.x.y" && exit 1)
	git tag -s $(VERSION) -m "Release $(VERSION)"
	git push origin $(VERSION)
	@echo "Tagged $(VERSION) — GitHub Actions will build and publish the release."
