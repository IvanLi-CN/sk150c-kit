# Makefile for SK150C Kit project
# STM32G431CBU6 adjustable power supply module control firmware

# Project configuration
CHIP = STM32G431CBUx
TARGET_DIR = target/thumbv7em-none-eabihf
BIN_NAME = sk150c-kit

# Build targets
.PHONY: build build-release clean check clippy fmt test
.PHONY: flash flash-release run run-release
.PHONY: attach attach-release reset reset-release reset-attach reset-attach-release
.PHONY: size size-release bloat bloat-release
.PHONY: help

# Default target
all: build

# Build commands
build:
	cargo build

build-release:
	cargo build --release

# Clean build artifacts
clean:
	cargo clean

# Code quality checks
check:
	cargo check

clippy:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

test:
	cargo test

# Flash and run commands
flash:
	cargo run

flash-release:
	cargo run --release

run:
	cargo run

run-release:
	cargo run --release

# Probe-rs debugging commands
attach:
	probe-rs attach --chip $(CHIP) $(TARGET_DIR)/debug/$(BIN_NAME)

attach-release:
	probe-rs attach --chip $(CHIP) $(TARGET_DIR)/release/$(BIN_NAME)

reset:
	probe-rs reset --chip $(CHIP)

reset-release:
	probe-rs reset --chip $(CHIP)

reset-attach: reset
	probe-rs attach --chip $(CHIP) $(TARGET_DIR)/debug/$(BIN_NAME)

reset-attach-release: reset-release
	probe-rs attach --chip $(CHIP) $(TARGET_DIR)/release/$(BIN_NAME)

# Size analysis
size: build
	cargo size --bin $(BIN_NAME)

size-release: build-release
	cargo size --release --bin $(BIN_NAME)

bloat: build
	cargo bloat --bin $(BIN_NAME)

bloat-release: build-release
	cargo bloat --release --bin $(BIN_NAME)

# Help target
help:
	@echo "Available targets:"
	@echo "  build           - Build debug version"
	@echo "  build-release   - Build release version"
	@echo "  clean           - Clean build artifacts"
	@echo "  check           - Check code without building"
	@echo "  clippy          - Run clippy linter"
	@echo "  fmt             - Format code"
	@echo "  test            - Run tests"
	@echo "  flash           - Flash debug version to MCU"
	@echo "  flash-release   - Flash release version to MCU"
	@echo "  run             - Build and run debug version"
	@echo "  run-release     - Build and run release version"
	@echo "  attach          - Attach debugger to debug binary"
	@echo "  attach-release  - Attach debugger to release binary"
	@echo "  reset           - Reset MCU"
	@echo "  reset-attach    - Reset MCU and attach debugger (debug)"
	@echo "  reset-attach-release - Reset MCU and attach debugger (release)"
	@echo "  size            - Show binary size (debug)"
	@echo "  size-release    - Show binary size (release)"
	@echo "  bloat           - Show code bloat analysis (debug)"
	@echo "  bloat-release   - Show code bloat analysis (release)"
	@echo "  help            - Show this help message"
