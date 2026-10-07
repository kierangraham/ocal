PREFIX ?= $(HOME)/.local
TARGET_DIR ?= target

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
	install -Dm755 "$(TARGET_DIR)/release/ocal" "$(PREFIX)/bin/ocal"
	install -Dm644 packaging/ocal.desktop "$(PREFIX)/share/applications/ocal.desktop"
	install -Dm644 assets/ocal.svg "$(PREFIX)/share/icons/hicolor/scalable/apps/ocal.svg"
	@command -v update-desktop-database >/dev/null && update-desktop-database "$(PREFIX)/share/applications" || true
uninstall:
	rm -f "$(PREFIX)/bin/ocal" "$(PREFIX)/share/applications/ocal.desktop" "$(PREFIX)/share/icons/hicolor/scalable/apps/ocal.svg"
clean:
	cargo clean --target-dir "$(TARGET_DIR)"
