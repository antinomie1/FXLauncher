# AGENTS.md — FXLauncher 研发指南

本项目是一个基于 Rust（2024 edition）开发的跨平台轻量化 Minecraft 启动器骨架。
UI 界面使用 Aegle 标记语言（`.aegle`）编写，并通过 Git 依赖引入 Aegle 库。

---

## 核心原则 (Core Principles)

1. **禁止过度工程 (No Over-Engineering)**
   - 保持架构直观、精简，聚焦启动器核心业务（路径、配置、账户、版本解析、Java 探测、命令构建与执行）。
   - 杜绝多余的抽象层、复杂的生命周期注入或臃肿的第三方框架。

2. **单 Crate 架构 (Single Crate)**
   - 整个项目为一个标准的单个 Cargo 包（非 Workspace）。
   - 通过 `src/lib.rs` 暴露核心库功能，`src/main.rs` 作为启动入口。

3. **测试与源码分离 (Tests Separated from Src)**
   - 业务逻辑与驱动放在 `src/`。
   - 所有集成测试与用例统一放在项目根目录的 `tests/` 目录中。

4. **单文件行数限制 (File Length Limit)**
   - 每个源文件及标记文件严格限制在 **1000 行以内**（建议保持在 100~300 行）。

5. **Aegle 标记语言驱动 UI (Aegle Markup UI)**
   - 界面布局与控件定义统一放在 `ui/` 下的 `.aegle` 文件中。
   - Rust 代码负责通过 `aegle::ui!` 宏编译界面并绑定业务回调，避免在 Rust 代码中硬编码复杂控件层级。

---

## 项目结构 (Directory Structure)

```text
fxlauncher/
├── AGENTS.md            # 本规范文件
├── Cargo.toml           # 项目定义 (Rust 2024, git aegle)
├── README.md            # 项目说明与快速开始
├── ui/                  # Aegle 标记语言界面描述
│   └── main.aegle       # 主窗口布局与控件定义
├── src/                 # 核心代码
│   ├── lib.rs           # 库导出与公共类型
│   ├── main.rs          # 启动入口与 CLI/GUI 运行
│   ├── paths.rs         # 跨平台路径计算 (.minecraft, Java)
│   ├── config.rs        # 启动器配置存储与加载 (JSON)
│   ├── account.rs       # 离线与在线账户数据结构及 UUID 生成
│   ├── java.rs          # Java 运行时检测与版本兼容性匹配
│   ├── version.rs       # Minecraft 客户端版本元数据解析 (version.json)
│   ├── launch.rs        # 启动参数组装与子进程构建
│   └── ui.rs            # Aegle 视图事件绑定与状态控制器
└── tests/               # 独立测试集合
    ├── paths_test.rs    # 跨平台路径解析测试
    ├── config_test.rs   # 配置读写与序列化测试
    ├── account_test.rs  # 账户与离线 UUID 生成测试
    ├── java_test.rs     # Java 版本输出解析与兼容测试
    ├── version_test.rs  # version.json 规则过滤与依赖解析测试
    └── launch_test.rs   # 启动命令与参数拼装测试
```

---

## 常用命令 (Commands)

```bash
# 代码检查
cargo check

# 运行全套测试 (tests/ 下的所有分离测试)
cargo test

# 构建并运行启动器
cargo run
```
