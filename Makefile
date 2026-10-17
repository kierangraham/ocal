PREFIX ?= $(HOME)/.local
DESTDIR ?=
TARGET_DIR ?= target

BINDIR := $(DESTDIR)$(PREFIX)/bin
APPDIR := $(DESTDIR)$(PREFIX)/share/applications
ICONDIR := $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps

.PHONY: run demo build check test lint install uninstall clean
run:
	cargo run --locked -p ocal-app --target-dir "$(TARGET_DIR)"
demo:
	cargo run --locked -p ocal-app --target-dir "$(TARGET_DIR)" -- --demo
build:
	cargo build --locked --release -p ocal-app --target-dir "$(TARGET_DIR)"
check:
	cargo check --locked --workspace --all-targets --target-dir "$(TARGET_DIR)"
test:
	cargo test --locked --workspace --target-dir "$(TARGET_DIR)"
lint:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets --target-dir "$(TARGET_DIR)" -- -D warnings
install: build
	install -Dm755 "$(TARGET_DIR)/release/ocal" "$(BINDIR)/ocal"
	install -Dm644 packaging/ocal.desktop "$(APPDIR)/ocal.desktop"
	install -Dm644 assets/ocal.svg "$(ICONDIR)/ocal.svg"
	@if [ -z "$(DESTDIR)" ] && command -v update-desktop-database >/dev/null; then update-desktop-database "$(APPDIR)" || true; fi
uninstall:
	rm -f "$(BINDIR)/ocal" "$(APPDIR)/ocal.desktop" "$(ICONDIR)/ocal.svg"
clean:
	cargo clean --target-dir "$(TARGET_DIR)"
