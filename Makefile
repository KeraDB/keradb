# KeraDB Makefile
# Build and package for multiple platforms

.PHONY: all build build-release test clean install uninstall package help

# Configuration
BINARY_NAME := keradb
VERSION := 0.1.0
BUILD_DIR := target
RELEASE_DIR := $(BUILD_DIR)/release
PACKAGE_DIR := packages

# Default target
all: build

# Build in debug mode
build:
	@echo "Building KeraDB (debug)..."
	cargo build

# Build in release mode
build-release:
	@echo "Building KeraDB (release)..."
	cargo build --release

# Run tests
test:
	@echo "Running tests..."
	cargo test

# Run benchmarks
bench:
	@echo "Running benchmarks..."
	cargo bench

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	rm -rf $(PACKAGE_DIR)

# Install locally (Linux/macOS)
install: build-release
	@echo "Installing KeraDB..."
	@mkdir -p ~/.local/bin
	@cp $(RELEASE_DIR)/$(BINARY_NAME) ~/.local/bin/
	@chmod +x ~/.local/bin/$(BINARY_NAME)
	@echo "✓ Installed to ~/.local/bin/$(BINARY_NAME)"
	@echo "Make sure ~/.local/bin is in your PATH"

# Uninstall (Linux/macOS)
uninstall:
	@echo "Uninstalling KeraDB..."
	@rm -f ~/.local/bin/$(BINARY_NAME)
	@echo "✓ Uninstalled"

# Package for distribution
package: package-linux package-macos package-windows

# Package for Linux
package-linux: build-release
	@echo "Packaging for Linux..."
	@mkdir -p $(PACKAGE_DIR)/linux
	@cp $(RELEASE_DIR)/$(BINARY_NAME) $(PACKAGE_DIR)/linux/
	@cp README.md LICENSE $(PACKAGE_DIR)/linux/ || true
	@tar -czf $(PACKAGE_DIR)/$(BINARY_NAME)-$(VERSION)-linux-x86_64.tar.gz \
		-C $(PACKAGE_DIR)/linux .
	@echo "✓ Created $(PACKAGE_DIR)/$(BINARY_NAME)-$(VERSION)-linux-x86_64.tar.gz"

# Package for macOS
package-macos:
	@echo "Packaging for macOS..."
	@echo "Note: Must be built on macOS"
	@mkdir -p $(PACKAGE_DIR)/macos
	@if [ -f "$(RELEASE_DIR)/$(BINARY_NAME)" ]; then \
		cp $(RELEASE_DIR)/$(BINARY_NAME) $(PACKAGE_DIR)/macos/; \
		cp README.md LICENSE $(PACKAGE_DIR)/macos/ || true; \
		tar -czf $(PACKAGE_DIR)/$(BINARY_NAME)-$(VERSION)-macos-x86_64.tar.gz \
			-C $(PACKAGE_DIR)/macos .; \
		echo "✓ Created $(PACKAGE_DIR)/$(BINARY_NAME)-$(VERSION)-macos-x86_64.tar.gz"; \
	else \
		echo "⚠ Binary not found. Build on macOS first."; \
	fi

# Package for Windows
package-windows:
	@echo "Packaging for Windows..."
	@echo "Note: Must be built on Windows or with cross-compilation"
	@mkdir -p $(PACKAGE_DIR)/windows
	@if [ -f "$(RELEASE_DIR)/$(BINARY_NAME).exe" ]; then \
		cp $(RELEASE_DIR)/$(BINARY_NAME).exe $(PACKAGE_DIR)/windows/; \
		cp README.md LICENSE scripts/install.ps1 scripts/install.bat $(PACKAGE_DIR)/windows/ || true; \
		cd $(PACKAGE_DIR)/windows && zip -r ../$(BINARY_NAME)-$(VERSION)-windows-x86_64.zip .; \
		echo "✓ Created $(PACKAGE_DIR)/$(BINARY_NAME)-$(VERSION)-windows-x86_64.zip"; \
	else \
		echo "⚠ Binary not found. Build on Windows first."; \
	fi

# Create Debian package
package-deb: build-release
	@echo "Creating Debian package..."
	@mkdir -p $(PACKAGE_DIR)/deb/DEBIAN
	@mkdir -p $(PACKAGE_DIR)/deb/usr/local/bin
	@mkdir -p $(PACKAGE_DIR)/deb/usr/share/doc/$(BINARY_NAME)
	@cp $(RELEASE_DIR)/$(BINARY_NAME) $(PACKAGE_DIR)/deb/usr/local/bin/
	@cp README.md $(PACKAGE_DIR)/deb/usr/share/doc/$(BINARY_NAME)/ || true
	@echo "Package: $(BINARY_NAME)" > $(PACKAGE_DIR)/deb/DEBIAN/control
	@echo "Version: $(VERSION)" >> $(PACKAGE_DIR)/deb/DEBIAN/control
	@echo "Architecture: amd64" >> $(PACKAGE_DIR)/deb/DEBIAN/control
	@echo "Maintainer: NoSQLite Team" >> $(PACKAGE_DIR)/deb/DEBIAN/control
	@echo "Description: Lightweight embedded NoSQL database" >> $(PACKAGE_DIR)/deb/DEBIAN/control
	@dpkg-deb --build $(PACKAGE_DIR)/deb $(PACKAGE_DIR)/$(BINARY_NAME)_$(VERSION)_amd64.deb
	@echo "✓ Created $(PACKAGE_DIR)/$(BINARY_NAME)_$(VERSION)_amd64.deb"

# Run the CLI
run:
	cargo run --release

# Development mode with auto-reload
dev:
	cargo watch -x run

# Format code
fmt:
	cargo fmt

# Lint code
lint:
	cargo clippy -- -D warnings

# Generate documentation
docs:
	cargo doc --no-deps --open

# Check code without building
check:
	cargo check

# Build for multiple targets (requires cross)
cross-build:
	@echo "Cross-compiling for multiple targets..."
	cross build --release --target x86_64-unknown-linux-gnu
	cross build --release --target x86_64-apple-darwin
	cross build --release --target x86_64-pc-windows-gnu

# Show help
help:
	@echo "NoSQLite Makefile"
	@echo ""
	@echo "Available targets:"
	@echo "  make build          - Build in debug mode"
	@echo "  make build-release  - Build in release mode"
	@echo "  make test           - Run tests"
	@echo "  make bench          - Run benchmarks"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make install        - Install locally (Linux/macOS)"
	@echo "  make uninstall      - Uninstall"
	@echo "  make package        - Package for all platforms"
	@echo "  make package-linux  - Package for Linux"
	@echo "  make package-macos  - Package for macOS"
	@echo "  make package-windows- Package for Windows"
	@echo "  make package-deb    - Create Debian package"
	@echo "  make run            - Run the CLI"
	@echo "  make fmt            - Format code"
	@echo "  make lint           - Lint code"
	@echo "  make docs           - Generate documentation"
	@echo "  make help           - Show this help"
