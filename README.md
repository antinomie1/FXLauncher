# FXLauncher (轻量化跨平台 Minecraft 启动器)

FXLauncher 是一个采用 **Rust 2024** 编写的跨平台轻量化 Minecraft 启动器骨架。界面使用 **Aegle** 保留模式标记语言（`.aegle`）构建，关注低资源占用、启动快速与清晰直观的架构设计。

---

## 特性 (Features)

- **现代 Rust 2024 架构**：单个轻量级 Crate，严格遵循“禁止过度工程”理念。
- **Aegle 标记语言驱动 UI**：使用 `.aegle` 描述主界面，通过 `aegle::ui!` 宏进行静态类型化编译绑定，支持动态主题切换（明亮/暗黑）。
- **跨平台适配**：自动探测 Windows、macOS 与 Linux 的标准 `.minecraft` 路径与 Classpath 分隔符。
- **Java 运行时探测与匹配**：自动扫描操作系统标准目录及 `JAVA_HOME`，解析 Java 主版本号并匹配 Minecraft 版本需求（如 1.20.5+ 需要 Java 21）。
- **版本规范支持**：完整兼容 Minecraft 现代规范（`arguments` 结构体、OS 规则匹配）与经典规范（`minecraftArguments`）。
- **离线与标准 UUID 生成**：符合 Minecraft 离线认证规范（MD5 派生 UUID v3）。
- **测试与源码分离**：所有集成测试均位于独立的 `tests/` 目录下。
- **极佳代码规范**：所有单一源文件均在 1000 行以内，结构扁平易读。

---

## 目录结构 (Directory Structure)

```text
fxlauncher/
├── AGENTS.md            # Agent 研发指引与设计契约（禁止过度工程）
├── Cargo.toml           # 项目依赖与 Rust 2024 配置
├── README.md            # 项目文档与快速上手
├── ui/
│   └── main.aegle       # Aegle 标记语言描述的启动器界面
├── src/
│   ├── lib.rs           # 模块导出与基础抽象
│   ├── main.rs          # 启动入口（支持 GUI 与 --headless 模式）
│   ├── paths.rs         # 跨平台路径计算与系统架构识别
│   ├── config.rs        # 启动器配置模型与 JSON 持久化
│   ├── account.rs       # 账户模型与离线 UUID 生成
│   ├── java.rs          # Java 探测、版本提取与版本兼容逻辑
│   ├── version.rs       # version.json 规则过滤与类路径装配
│   ├── launch.rs        # 启动命令与 JVM 参数组装
│   └── ui.rs            # Aegle 界面事件绑定与控制器
└── tests/
    ├── paths_test.rs    # 路径解析测试
    ├── account_test.rs  # 账户与 UUID 测试
    ├── config_test.rs   # 配置读写测试
    ├── java_test.rs     # Java 输出解析与兼容测试
    ├── version_test.rs  # 版本 JSON 与规则测试
    └── launch_test.rs   # 启动命令与参数拼装测试
```

---

## 快速开始 (Quick Start)

### 编译检查

```bash
cargo check
```

### 运行全套测试

```bash
cargo test
```

### 运行启动器

- **启动图形界面 (GUI)**：
  ```bash
  cargo run
  ```

- **无头/CLI 模式 (Headless / 无图形环境)**：
  ```bash
  cargo run -- --headless
  ```

- **查看帮助**：
  ```bash
  cargo run -- --help
  ```

---

## 开源协议

本项目采用 MIT 或 Apache-2.0 双重许可证。
