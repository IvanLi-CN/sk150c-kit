#!/bin/bash

# SK150C Kit Development Environment Setup Script
# This script configures the development environment for the SK150C project
# It does NOT install base tools like Rust or Bun - those should be installed separately

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print functions
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_step() {
    echo -e "\n${BLUE}==>${NC} $1"
}

# Check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check required tools
check_tools() {
    print_step "Checking required tools..."
    
    local missing_tools=()
    
    # Check Rust toolchain
    if ! command_exists cargo; then
        missing_tools+=("cargo (Rust)")
    fi
    
    if ! command_exists rustup; then
        missing_tools+=("rustup")
    fi
    
    # Check Lefthook
    if ! command_exists lefthook; then
        missing_tools+=("lefthook")
    fi
    
    # Check Bun
    if ! command_exists bunx; then
        missing_tools+=("bunx (Bun)")
    fi
    
    # Check probe-rs (optional but recommended)
    if ! command_exists probe-rs; then
        print_warning "probe-rs not found - hardware debugging will not be available"
        print_info "Install with: cargo install probe-rs --features cli"
    else
        print_success "probe-rs found"
    fi
    
    if [ ${#missing_tools[@]} -ne 0 ]; then
        print_error "Missing required tools:"
        for tool in "${missing_tools[@]}"; do
            echo "  - $tool"
        done
        echo ""
        print_info "Please install the missing tools and run this script again."
        print_info "Installation guides:"
        print_info "  - Rust: https://rustup.rs/"
        print_info "  - Lefthook: https://github.com/evilmartians/lefthook"
        print_info "  - Bun: https://bun.sh/"
        exit 1
    fi
    
    print_success "All required tools are available"
}

# Configure Rust toolchain
setup_rust() {
    print_step "Setting up Rust toolchain..."
    
    # Add target for STM32G431CBU6
    print_info "Adding thumbv7em-none-eabihf target..."
    rustup target add thumbv7em-none-eabihf
    
    # Install required components
    print_info "Installing rustfmt component..."
    rustup component add rustfmt
    
    print_info "Installing clippy component..."
    rustup component add clippy
    
    print_info "Installing llvm-tools-preview component..."
    rustup component add llvm-tools-preview
    
    print_success "Rust toolchain configured"
}

# Install cargo tools
install_cargo_tools() {
    print_step "Installing cargo tools..."
    
    # Check if cargo-binutils is installed
    if ! cargo size --version >/dev/null 2>&1; then
        print_info "Installing cargo-binutils..."
        cargo install cargo-binutils
    else
        print_success "cargo-binutils already installed"
    fi
    
    # Check if cargo-bloat is installed
    if ! command_exists cargo-bloat; then
        print_info "Installing cargo-bloat..."
        cargo install cargo-bloat
    else
        print_success "cargo-bloat already installed"
    fi
}

# Setup git hooks
setup_git_hooks() {
    print_step "Setting up git hooks..."
    
    print_info "Installing lefthook hooks..."
    lefthook install
    
    print_success "Git hooks configured"
}

# Verify environment
verify_environment() {
    print_step "Verifying environment..."
    
    # Test Rust compilation
    print_info "Testing Rust compilation..."
    if cargo check --target thumbv7em-none-eabihf >/dev/null 2>&1; then
        print_success "Rust compilation test passed"
    else
        print_error "Rust compilation test failed"
        return 1
    fi
    
    # Test git hooks
    print_info "Testing git hooks..."
    if lefthook run pre-commit >/dev/null 2>&1; then
        print_success "Git hooks test passed"
    else
        print_warning "Git hooks test had issues (this may be normal if no files are staged)"
    fi
    
    # Test JavaScript tools
    print_info "Testing JavaScript tools..."
    if bunx markdownlint-cli2 --version >/dev/null 2>&1; then
        print_success "markdownlint-cli2 available"
    else
        print_warning "markdownlint-cli2 test failed"
    fi
    
    if echo "test: sample message" | bunx commitlint >/dev/null 2>&1; then
        print_success "commitlint available"
    else
        print_warning "commitlint test failed (this may be normal)"
    fi
}

# Main function
main() {
    echo "SK150C Kit Development Environment Setup"
    echo "========================================"
    
    check_tools
    setup_rust
    install_cargo_tools
    setup_git_hooks
    verify_environment
    
    echo ""
    print_success "Development environment setup completed!"
    echo ""
    print_info "You can now:"
    print_info "  - Build the project: make build"
    print_info "  - Run code checks: make clippy"
    print_info "  - Format code: make fmt"
    print_info "  - Flash to hardware: make run"
    print_info "  - Analyze binary size: make size"
    echo ""
    print_info "For more commands, run: make help"
}

# Show help
show_help() {
    echo "SK150C Kit Development Environment Setup Script"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  -h, --help    Show this help message"
    echo ""
    echo "This script configures the development environment for the SK150C project."
    echo "It assumes that Rust, Bun, and Lefthook are already installed."
    echo ""
    echo "What this script does:"
    echo "  1. Checks for required tools"
    echo "  2. Configures Rust toolchain (adds target and components)"
    echo "  3. Installs necessary cargo tools"
    echo "  4. Sets up git hooks with Lefthook"
    echo "  5. Verifies the environment"
}

# Parse command line arguments
case "${1:-}" in
    -h|--help)
        show_help
        exit 0
        ;;
    "")
        main
        ;;
    *)
        print_error "Unknown option: $1"
        echo "Use -h or --help for usage information."
        exit 1
        ;;
esac
