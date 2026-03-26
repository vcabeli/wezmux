APP_DIR ?= /Applications/Wezmux.app

.PHONY: all fmt build check test docs servedocs install bundle

all: build

test:
	cargo nextest run
	cargo nextest run -p wezterm-escape-parser # no_std by default

check:
	cargo check
	cargo check -p wezterm-escape-parser
	cargo check -p wezterm-cell
	cargo check -p wezterm-surface
	cargo check -p wezterm-ssh

build:
	cargo build $(BUILD_OPTS) -p wezterm
	cargo build $(BUILD_OPTS) -p wezterm-gui
	cargo build $(BUILD_OPTS) -p wezterm-mux-server
	cargo build $(BUILD_OPTS) -p strip-ansi-escapes

fmt:
	cargo +nightly fmt

docs:
	ci/build-docs.sh

servedocs:
	ci/build-docs.sh serve

install:
	cargo build --release -p wezterm -p wezterm-gui -p wezterm-mux-server -p strip-ansi-escapes
	mkdir -p $(APP_DIR)/Contents/MacOS
	mkdir -p $(APP_DIR)/Contents/Resources
	cp assets/macos/WezTerm.app/Contents/Info.plist $(APP_DIR)/Contents/Info.plist
	cp assets/macos/WezTerm.app/Contents/Resources/terminal.icns $(APP_DIR)/Contents/Resources/terminal.icns
	cp target/release/wezterm $(APP_DIR)/Contents/MacOS/wezterm
	cp target/release/wezterm-mux-server $(APP_DIR)/Contents/MacOS/wezterm-mux-server
	cp target/release/strip-ansi-escapes $(APP_DIR)/Contents/MacOS/strip-ansi-escapes
	cp target/release/wezterm-gui /tmp/wezterm-gui
	codesign --force --sign - /tmp/wezterm-gui
	cp /tmp/wezterm-gui $(APP_DIR)/Contents/MacOS/wezterm-gui
	rm /tmp/wezterm-gui
	@echo "Wezmux.app installed to $(APP_DIR)"

bundle:
	cargo build --release -p wezterm -p wezterm-gui -p wezterm-mux-server -p strip-ansi-escapes
	mkdir -p target/Wezmux.app/Contents/MacOS
	cp target/release/wezterm-gui target/Wezmux.app/Contents/MacOS/wezterm-gui
	cp target/release/wezterm target/Wezmux.app/Contents/MacOS/wezterm
	cp target/release/wezterm-mux-server target/Wezmux.app/Contents/MacOS/wezterm-mux-server
	cp target/release/strip-ansi-escapes target/Wezmux.app/Contents/MacOS/strip-ansi-escapes
	cp assets/macos/WezTerm.app/Contents/Info.plist target/Wezmux.app/Contents/Info.plist
	mkdir -p target/Wezmux.app/Contents/Resources
	cp assets/macos/WezTerm.app/Contents/Resources/terminal.icns target/Wezmux.app/Contents/Resources/terminal.icns
	codesign --force --sign - target/Wezmux.app/Contents/MacOS/wezterm-gui
	@echo "Wezmux.app bundle ready at target/Wezmux.app"
