# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

SK150C Kit is an embedded Rust firmware for STM32G431CBU6 microcontroller that implements a complete power management solution for adjustable power supply modules. The firmware provides USB PD sink functionality, voltage/current monitoring, power output control, and user interface management through buttons and LEDs.

## Build Commands

### Primary Build Commands (Use Makefile)
- `make build` - Build debug version  
- `make build-release` - Build release version
- `make flash` - Build and flash debug version to MCU (equivalent to `cargo run`)
- `make flash-release` - Build and flash release version
- `make test` - Run tests
- `make check` - Check code without building
- `make clippy` - Run clippy linter (configured to fail on warnings)
- `make fmt` - Format code

### Development Workflow
- `make all` - Default target, builds debug version
- `make clean` - Clean build artifacts

### Hardware-Specific Commands
- `make attach` / `make attach-release` - Attach debugger using probe-rs
- `make reset` - Reset MCU using probe-rs  
- `make size` / `make size-release` - Show binary size analysis
- `make bloat` / `make bloat-release` - Show code bloat analysis

## Architecture Overview

### Core Architecture
The firmware is built on **embassy-rs**, an async embedded framework for STM32. Key architectural patterns:

- **Async Task-Based Design**: Multiple concurrent tasks managed by embassy-executor
- **Channel-Based Communication**: Inter-task communication via embassy-sync channels and watches
- **Modular Component Structure**: Each major function (ADC, buttons, power management) is a separate module

### Key Modules and Responsibilities

- **`main.rs`**: System initialization, task spawning, and peripheral setup
- **`shared.rs`**: Global communication channels and constants used across modules
- **`app_manager.rs`**: Main application logic and power management state machine
- **`power.rs`**: USB PD sink implementation using usbpd crate
- **`adc_reader.rs`**: ADC voltage/current/temperature monitoring with calibration
- **`button/`**: Button input handling with debouncing and press detection
- **`vbus_manager.rs`**: USB-C output power switch control and LED management
- **`power_output.rs`**: Power output control logic
- **`config_manager.rs`**: Configuration management and persistence
- **`fan_manager.rs`**: Fan control system
- **`types.rs`**: Common type definitions

### Communication Architecture
The system uses embassy-sync primitives for inter-task communication:
- **PubSubChannel**: ADC readings broadcast to multiple consumers
- **Watch**: State sharing (VBUS voltage, VIN voltage, switch states)  
- **Channel**: Point-to-point communication (config requests, PD errors)

### Hardware Abstraction
- **Target**: `thumbv7em-none-eabihf` (Cortex-M4F)
- **Chip**: STM32G431CBU6
- **Key Peripherals**: ADC1/2, UCPD1 (USB PD), TIM1/4 (PWM), GPIO, USART2

## Development Guidelines

### Code Style and Tooling
- Code formatting enforced via `cargo fmt` (rustfmt.toml configuration)
- Linting enforced via `cargo clippy` with warnings-as-errors
- Git hooks managed by lefthook (pre-commit: fmt, clippy, markdown-lint)
- Commit messages follow Conventional Commits specification
- No standard library (`#![no_std]`, `#![no_main]`)

### Testing Strategy
- Unit tests in `src/tests/` directory
- Hardware-in-the-loop testing capability
- Mock implementations available for button inputs (`button/mock_impl.rs`)

### Key Dependencies
- **embassy-rs**: Async embedded framework (specific git revision pinned)
- **usbpd**: USB PD protocol stack  
- **defmt/defmt-rtt**: Efficient logging system with RTT output
- **embedded-hal**: Hardware abstraction layer
- **static_cell**: Static memory allocation for async tasks

### Memory Management
- Uses `embedded-alloc` with heap allocator
- Optimized build profiles for code size (opt-level = "z" in dev, LTO enabled)
- Static memory allocation patterns for embedded constraints

## Hardware Context

### Pin Configuration (Key Pins)
- **PA0**: VBUS voltage detection (ADC1_IN1)
- **PA1**: VIN voltage detection (ADC2_IN2) 
- **PA15**: VIN_EN power input control
- **PB7**: VBUS_EN USB-C power output switch
- **PB5**: VBUS_LED dual-color LED control
- **PA8**: POWER_LED with PWM breathing effect (TIM1_CH1)
- **PB8**: POWER_KEY button input
- **PB0**: NTC temperature sensor (ADC1_IN15)

### Power Management System
Implements a dual-state system (Standby/Working) with:
- Automatic state transitions based on button input (1.5s long press)
- LED breathing effects in standby mode
- Synchronized hardware control across power switches and indicators

## Debugging and Development

### Logging System
- Uses defmt for efficient logging with RTT transport
- Debug output via `probe-rs attach --rtt` or `make rtt` command
- Log statements: `defmt::info!()`, `defmt::warn!()`, etc.

### Build Profiles
- **Debug**: Optimized for size (opt-level = "z") but includes debug info
- **Release**: Full optimization with LTO enabled
- **Test**: Standard debug configuration for testing

### Common Development Tasks
- Hardware debugging requires probe-rs and compatible debugger
- Flash via SWD interface (PA13/PA14)
- RTT logging for real-time debug output
- Size analysis tools available for memory optimization