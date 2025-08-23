# SK150C Kit 开发指南

本文档为 SK150C 套件项目的开发者提供详细的开发指南。

## 快速开始

### 1. 环境设置

#### 前置要求

首先，确保您已安装以下基础开发工具：

- **Rust 工具链**：从 [rustup.rs](https://rustup.rs/) 安装
- **Bun**：从 [bun.sh](https://bun.sh/) 安装（用于 JavaScript 工具）
- **Lefthook**：从 [github.com/evilmartians/lefthook](https://github.com/evilmartians/lefthook) 安装
- **probe-rs**：用于硬件调试（可选但推荐）

#### 自动化设置

使用提供的设置脚本来配置您的开发环境：

```bash
# 运行开发环境设置脚本
./scripts/setup-dev.sh
```

此脚本将：
- 检查必需的工具
- 配置 Rust 工具链（添加目标和组件）
- 安装必要的 cargo 工具（cargo-binutils、cargo-bloat）
- 使用 Lefthook 设置 git hooks
- 验证环境

#### 手动设置（备选方案）

如果您更喜欢手动设置或脚本失败：

```bash
# 安装 Rust 目标
rustup target add thumbv7em-none-eabihf

# 安装 Rust 组件
rustup component add rustfmt clippy llvm-tools-preview

# 安装 cargo 工具
cargo install probe-rs --features cli
cargo install cargo-binutils cargo-bloat

# 设置 git hooks
lefthook install
```

### 2. 构建项目

```bash
# 使用构建脚本
./scripts/build.sh build          # Debug 版本
./scripts/build.sh release        # Release 版本
./scripts/build.sh all            # 完整流程

# 或者使用 Makefile
make build                        # Debug 版本
make build-release               # Release 版本
make all                         # 完整流程
```

### 3. 代码检查

```bash
# 格式化代码
make fmt

# 运行 Clippy 检查
make clippy

# 运行所有检查
make check
```

## 项目结构

```
sk150c-kit/
├── src/                         # 源代码
│   ├── main.rs                  # 主程序入口
│   ├── adc_reader.rs           # ADC 读取模块
│   ├── app_manager.rs          # 应用管理器
│   ├── button.rs               # 按键输入处理
│   ├── comp.rs                 # 比较器和保护功能
│   ├── config_manager.rs       # 配置管理
│   ├── power.rs                # USB PD 电源输入
│   ├── power_output.rs         # 电源输出控制
│   ├── shared.rs               # 共享定义
│   ├── types.rs                # 类型定义
│   └── usb.rs                  # USB 通信
├── models/                      # 3D 模型文件
│   ├── README.md               # 模型说明文档
│   └── *.step, *.FCStd         # 外壳设计文件
├── docs/                        # 文档
│   └── SK150C-Digital-Power-Supply-Manual.pdf
├── scripts/                     # 开发脚本
│   ├── build.sh                # 构建脚本
│   └── setup-dev.sh            # 环境设置脚本
├── .github/workflows/           # CI/CD 配置
│   └── ci.yml                  # GitHub Actions
├── sk150c-kit.ioc              # STM32CubeMX 配置
├── Cargo.toml                  # Rust 项目配置
├── Makefile                    # 构建工具
└── README.md                   # 项目说明
```

## 开发工作流

### 日常开发

1. **创建功能分支**
   ```bash
   git checkout -b feature/new-feature
   ```

2. **开发和测试**
   ```bash
   # 开发工作流 (格式化、检查、构建)
   make dev
   
   # 或者分步执行
   make fmt          # 格式化代码
   make check        # 代码检查
   make build        # 构建项目
   ```

3. **提交代码**
   ```bash
   git add .
   git commit -m "feat: add new feature"
   git push origin feature/new-feature
   ```

### 硬件调试

1. **连接调试器**
   ```bash
   # 列出连接的调试器
   make list-probes
   
   # 显示芯片信息
   make chip-info
   ```

2. **烧录固件**
   ```bash
   # 快速构建并烧录
   make quick
   
   # 或者分步执行
   make build        # 构建
   make flash        # 烧录
   ```

3. **调试会话**
   ```bash
   # 启动调试会话
   make debug
   
   # 连接 RTT 输出
   make rtt
   ```

## 代码规范

### Rust 代码风格

- 使用 `cargo fmt` 进行代码格式化
- 遵循 Rust 官方代码风格指南
- 使用 `cargo clippy` 进行代码质量检查

### 提交信息规范

使用 [Conventional Commits](https://www.conventionalcommits.org/) 规范：

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

类型说明：
- `feat`: 新功能
- `fix`: 错误修复
- `docs`: 文档更新
- `style`: 代码格式调整
- `refactor`: 代码重构
- `test`: 测试相关
- `chore`: 构建过程或辅助工具的变动

示例：
```
feat(power): add USB PD PPS support
fix(adc): correct voltage reading calibration
docs: update hardware connection guide
```

## 硬件配置

### 引脚映射

| 引脚 | 功能 | 描述 |
|------|------|------|
| PA0 | VOUT_SN | 输出电压检测 (ADC1_IN1) |
| PA1 | VIN_SN | 输入电压检测 (ADC2_IN2) |
| PA15 | VIN_CE | 输入控制使能 |
| PB5 | VBUS_CE | VBUS控制使能 |
| PB7 | VBUS_LED | VBUS状态LED (TIM4_CH2) |
| PA8 | POWER_LED | 电源状态LED (TIM1_CH1) |
| PB0 | NTC | 温度检测 (ADC1_IN15) |

### USB PD 接口

| 引脚 | 功能 | 描述 |
|------|------|------|
| PA9/PA10 | UCPD1_DBCC1/DBCC2 | Dead Battery |
| PB4/PB6 | UCPD1_CC1/CC2 | USB PD通信 |
| PA11/PA12 | USB_DM/DP | USB通信 |

## 调试技巧

### 使用 defmt 日志

```rust
use defmt::info;

info!("Voltage: {}V, Current: {}A", voltage, current);
```

### RTT 输出

项目使用 RTT (Real-Time Transfer) 进行日志输出：

```bash
# 连接 RTT 输出
make rtt

# 或者使用 probe-rs 直接连接
probe-rs attach --chip STM32G431CBUx --rtt
```

### 常见问题

1. **编译错误**
   - 确保已安装正确的 Rust 工具链
   - 检查 embassy 依赖版本

2. **烧录失败**
   - 检查调试器连接
   - 确认芯片型号正确

3. **RTT 无输出**
   - 检查 defmt 配置
   - 确认调试器支持 RTT

## 测试

### 单元测试

```bash
# 运行测试
make test

# 或者使用 cargo
cargo test
```

### 硬件在环测试

项目支持硬件在环测试，需要连接实际硬件：

```bash
# 构建并烧录测试固件
make build
make flash

# 运行硬件测试
# (具体测试流程待实现)
```

## 发布流程

### 版本发布

1. **更新版本号**
   ```bash
   # 更新 Cargo.toml 中的版本号
   vim Cargo.toml
   ```

2. **创建发布标签**
   ```bash
   git tag -a v1.0.0 -m "Release version 1.0.0"
   git push origin v1.0.0
   ```

3. **GitHub Actions 自动构建**
   - CI/CD 会自动构建固件
   - 生成的二进制文件会作为 artifacts 上传

## 贡献指南

1. Fork 项目
2. 创建功能分支
3. 提交更改
4. 创建 Pull Request
5. 等待代码审查

## 许可证

本项目采用 MIT 许可证，详见 [LICENSE](LICENSE) 文件。
