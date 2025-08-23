# SK150C Kit Developer Guide

This document provides a detailed development guide for developers of the SK150C kit project.

## Quick Start

### 1. Environment Setup

#### Prerequisites

First, ensure you have the base development tools installed:

- **Rust toolchain**: Install from [rustup.rs](https://rustup.rs/)
- **Bun**: Install from [bun.sh](https://bun.sh/) (for JavaScript tools)
- **Lefthook**: Install from [github.com/evilmartians/lefthook](https://github.com/evilmartians/lefthook)
- **probe-rs**: For hardware debugging (optional but recommended)

#### Automated Setup

Use the provided setup script to configure your development environment:

```bash
# Run the development environment setup script
./scripts/setup-dev.sh
```

This script will:

- Check for required tools
- Configure Rust toolchain (add target and components)
- Install necessary cargo tools (cargo-binutils, cargo-bloat)
- Set up git hooks with Lefthook
- Verify the environment

#### Manual Setup (Alternative)

If you prefer manual setup or the script fails:

```bash
# Install Rust target
rustup target add thumbv7em-none-eabihf

# Install Rust components
rustup component add rustfmt clippy llvm-tools-preview

# Install cargo tools
cargo install probe-rs --features cli
cargo install cargo-binutils cargo-bloat

# Setup git hooks
lefthook install
```

### 2. Build Project

```bash
# Use build script
./scripts/build.sh build          # Debug version
./scripts/build.sh release        # Release version
./scripts/build.sh all            # Complete workflow

# Or use Makefile
make build                        # Debug version
make build-release               # Release version
make all                         # Complete workflow
```

### 3. Code Checking

```bash
# Format code
make fmt

# Run Clippy checks
make clippy

# Run all checks
make check
```

## Project Structure

```
sk150c-kit/
├── src/                         # Source code
│   ├── main.rs                  # Main program entry
│   ├── adc_reader.rs           # ADC reading module
│   ├── app_manager.rs          # Application manager
│   ├── button.rs               # Button input processing
│   ├── comp.rs                 # Comparator and protection functions
│   ├── config_manager.rs       # Configuration management
│   ├── power.rs                # USB PD power input
│   ├── power_output.rs         # Power output control
│   ├── shared.rs               # Shared definitions
│   ├── types.rs                # Type definitions
│   └── usb.rs                  # USB communication
├── models/                      # 3D model files
│   ├── README.md               # Model documentation
│   └── *.step, *.FCStd         # Shell design files
├── docs/                        # Documentation
│   └── SK150C-Digital-Power-Supply-Manual.pdf
├── scripts/                     # Development scripts
│   ├── build.sh                # Build script
│   └── setup-dev.sh            # Environment setup script
├── .github/workflows/           # CI/CD configuration
│   └── ci.yml                  # GitHub Actions
├── sk150c-kit.ioc              # STM32CubeMX configuration
├── Cargo.toml                  # Rust project configuration
├── Makefile                    # Build tools
└── README.md                   # Project description
```

## Development Workflow

### Daily Development

1. **Create Feature Branch**
   ```bash
   git checkout -b feature/new-feature
   ```

2. **Development and Testing**
   ```bash
   # Development workflow (format, check, build)
   make dev
   
   # Or execute step by step
   make fmt          # Format code
   make check        # Code checking
   make build        # Build project
   ```

3. **Commit Code**
   ```bash
   git add .
   git commit -m "feat: add new feature"
   git push origin feature/new-feature
   ```

### Hardware Debugging

1. **Connect Debugger**
   ```bash
   # List connected debuggers
   make list-probes
   
   # Show chip information
   make chip-info
   ```

2. **Flash Firmware**
   ```bash
   # Quick build and flash
   make quick
   
   # Or execute step by step
   make build        # Build
   make flash        # Flash
   ```

3. **Debug Session**
   ```bash
   # Start debug session
   make debug
   
   # Connect RTT output
   make rtt
   ```

## Code Standards

### Rust Code Style

- Use `cargo fmt` for code formatting
- Follow Rust official code style guide
- Use `cargo clippy` for code quality checking

### Commit Message Standards

Use [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

Type descriptions:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation update
- `style`: Code formatting adjustment
- `refactor`: Code refactoring
- `test`: Test related
- `chore`: Build process or auxiliary tool changes

Examples:
```
feat(power): add USB PD PPS support
fix(adc): correct voltage reading calibration
docs: update hardware connection guide
```

## Hardware Configuration

### Pin Mapping

| Pin | Function | Description |
|-----|----------|-------------|
| PA0 | VOUT_SN | Output voltage detection (ADC1_IN1) |
| PA1 | VIN_SN | Input voltage detection (ADC2_IN2) |
| PA15 | VIN_CE | Input control enable |
| PB5 | VBUS_CE | VBUS control enable |
| PB7 | VBUS_LED | VBUS status LED (TIM4_CH2) |
| PA8 | POWER_LED | Power status LED (TIM1_CH1) |
| PB0 | NTC | Temperature detection (ADC1_IN15) |

### USB PD Interface

| Pin | Function | Description |
|-----|----------|-------------|
| PA9/PA10 | UCPD1_DBCC1/DBCC2 | Dead Battery |
| PB4/PB6 | UCPD1_CC1/CC2 | USB PD communication |
| PA11/PA12 | USB_DM/DP | USB communication |

## Debugging Tips

### Using defmt Logging

```rust
use defmt::info;

info!("Voltage: {}V, Current: {}A", voltage, current);
```

### RTT Output

The project uses RTT (Real-Time Transfer) for log output:

```bash
# Connect RTT output
make rtt

# Or use probe-rs directly
probe-rs attach --chip STM32G431CBUx --rtt
```

### Common Issues

1. **Compilation Errors**
   - Ensure correct Rust toolchain is installed
   - Check embassy dependency versions

2. **Flash Failures**
   - Check debugger connection
   - Confirm correct chip model

3. **No RTT Output**
   - Check defmt configuration
   - Confirm debugger supports RTT

## Testing

### Unit Testing

```bash
# Run tests
make test

# Or use cargo
cargo test
```

### Hardware-in-the-Loop Testing

The project supports hardware-in-the-loop testing, requiring actual hardware connection:

```bash
# Build and flash test firmware
make build
make flash

# Run hardware tests
# (Specific test procedures to be implemented)
```

## Release Process

### Version Release

1. **Update Version Number**
   ```bash
   # Update version number in Cargo.toml
   vim Cargo.toml
   ```

2. **Create Release Tag**
   ```bash
   git tag -a v1.0.0 -m "Release version 1.0.0"
   git push origin v1.0.0
   ```

3. **GitHub Actions Automatic Build**
   - CI/CD will automatically build firmware
   - Generated binary files will be uploaded as artifacts

## Contributing Guidelines

1. Fork the project
2. Create a feature branch
3. Commit changes
4. Create a Pull Request
5. Wait for code review

## License

This project is licensed under the MIT License, see [LICENSE](LICENSE) file for details.

---

*For Chinese documentation, see DEVELOPER_GUIDE.zh.md*
