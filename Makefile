APP_DIR ?= /Applications/Wezmux.app

.PHONY: all fmt build check test install install-codex-hooks install-remote bundle

all: build

test:
	@if cargo nextest --version >/dev/null 2>&1; then \
		cargo nextest run; \
		cargo nextest run -p wezterm-escape-parser; \
	else \
		echo "cargo-nextest not found; falling back to cargo test"; \
		cargo test --workspace; \
		cargo test -p wezterm-escape-parser; \
	fi

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

install:
	cargo build --release -p wezterm -p wezterm-gui -p wezterm-mux-server -p strip-ansi-escapes
	rm -rf $(APP_DIR)
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
	cp -R bin $(APP_DIR)/Contents/Resources/bin
	ln -s ../Resources/bin $(APP_DIR)/Contents/MacOS/bin
	chmod +x $(APP_DIR)/Contents/Resources/bin/claude $(APP_DIR)/Contents/Resources/bin/wezmux $(APP_DIR)/Contents/Resources/bin/wezmux-install-remote $(APP_DIR)/Contents/Resources/bin/hooks/*.sh $(APP_DIR)/Contents/Resources/bin/hooks/codex/*.sh $(APP_DIR)/Contents/Resources/bin/install-codex-hooks.sh
	xattr -cr $(APP_DIR)
	@echo "Wezmux.app installed to $(APP_DIR)"
	@echo ""
	@echo "Optional: put the wezmux launcher on your PATH:"
	@echo "  mkdir -p ~/.local/bin && ln -sf $(APP_DIR)/Contents/Resources/bin/wezmux ~/.local/bin/wezmux"
	@echo "  (or system-wide: sudo ln -sf $(APP_DIR)/Contents/Resources/bin/wezmux /usr/local/bin/wezmux)"
	@echo "Optional: run 'make install-codex-hooks' to set up Codex integration"

install-codex-hooks:
	$(APP_DIR)/Contents/Resources/bin/install-codex-hooks.sh

# Install wezmux on a remote host so that `wezmux --ssh HOST` can run the
# session there. See docs/remote.md.
install-remote:
	@test -n "$(HOST)" || (echo "usage: make install-remote HOST=[user@]host"; exit 1)
	bin/wezmux-install-remote $(HOST)

bundle:
	cargo build --release -p wezterm -p wezterm-gui -p wezterm-mux-server -p strip-ansi-escapes
	rm -rf target/Wezmux.app
	mkdir -p target/Wezmux.app/Contents/MacOS
	cp target/release/wezterm-gui target/Wezmux.app/Contents/MacOS/wezterm-gui
	cp target/release/wezterm target/Wezmux.app/Contents/MacOS/wezterm
	cp target/release/wezterm-mux-server target/Wezmux.app/Contents/MacOS/wezterm-mux-server
	cp target/release/strip-ansi-escapes target/Wezmux.app/Contents/MacOS/strip-ansi-escapes
	cp assets/macos/WezTerm.app/Contents/Info.plist target/Wezmux.app/Contents/Info.plist
	mkdir -p target/Wezmux.app/Contents/Resources
	cp assets/macos/WezTerm.app/Contents/Resources/terminal.icns target/Wezmux.app/Contents/Resources/terminal.icns
	cp -R bin target/Wezmux.app/Contents/Resources/bin
	ln -s ../Resources/bin target/Wezmux.app/Contents/MacOS/bin
	chmod +x target/Wezmux.app/Contents/Resources/bin/claude target/Wezmux.app/Contents/Resources/bin/wezmux target/Wezmux.app/Contents/Resources/bin/wezmux-install-remote target/Wezmux.app/Contents/Resources/bin/hooks/*.sh target/Wezmux.app/Contents/Resources/bin/hooks/codex/*.sh target/Wezmux.app/Contents/Resources/bin/install-codex-hooks.sh
	codesign --force --sign - target/Wezmux.app/Contents/MacOS/wezterm-gui
	@echo "Wezmux.app bundle ready at target/Wezmux.app"
