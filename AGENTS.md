# AGENTS.md — FXLauncher 研发指南

本项目是一个基于 Rust（2024 edition）开发的跨平台轻量化 Minecraft 启动器核心后端。

---

## 核心原则 (Core Principles)

1. **禁止过度工程 (No Over-Engineering)**
   - 保持架构直观、扁平，聚焦启动器核心后端能力。
   - 杜绝多余的抽象层或复杂的生命周期注入。

2. **单 Crate 架构 (Single Crate)**
   - 项目为一个标准的单 Cargo 包（非 Workspace）。
   - 通过 `src/lib.rs` 暴露核心库功能。

3. **测试与源码分离 (Tests Separated from Src)**
   - 业务逻辑放在 `src/`。
   - 所有集成测试与用例统一放在项目根目录的 `tests/` 目录中。

4. **单文件行数限制 (File Length Limit)**
   - 每个源文件严格限制在 **1000 行以内**（通常保持在 100~350 行）。

5. **版本隔离与非阻塞任务设计**
   - 支持将游戏工作目录绑定至对应版本子目录（`versions/<version>/`），每个版本具有独立的 `mods/`、`config/`、`saves/`。
   - 网络下载等耗时操作在后台工作线程执行，通过全局任务管理器跟踪状态、进度并支持暂停、恢复与取消。

---

## 项目结构 (Directory Structure)

```text
fxlauncher/
├── AGENTS.md            # 本规范文件
├── Cargo.toml           # 项目定义 (Rust 2024)
├── README.md            # 项目说明与测试指引
├── src/                 # 后端核心源码
│   ├── lib.rs           # 库导出与公共模块暴露
│   ├── main.rs          # 最小入口
│   ├── env/             # 环境探测与平台路径
│   │   ├── paths.rs     # 跨平台路径计算与版本子目录辅助函数
│   │   └── java.rs      # Java 探测、版本提取与版本兼容匹配
│   ├── game/            # 游戏核心模型与运行
│   │   ├── account.rs   # 离线与在线账户模型、UUID 派生
│   │   ├── config.rs    # 启动器配置存储与持久化 (JSON)
│   │   ├── version.rs   # version.json 解析与类路径装配
│   │   └── launch.rs    # 启动命令组装、JVM 参数与进程管理
│   ├── net/             # 网络、任务管理与内容下载
│   │   ├── client.rs    # HTTP 请求与参数编码轻量封装
│   │   ├── task.rs      # 全局非阻塞任务管理与 SHA-1 校验
│   │   ├── download.rs  # 游戏核心与版本元数据下载 (官方 + BMCLAPI)
│   │   └── mods.rs      # 模组搜索与下载管理 (Modrinth + CurseForge)
│   └── diag/            # 诊断、日志与崩溃分析
│       ├── log.rs       # 日志行解析与错误过滤
│       ├── crash.rs     # 标准崩溃报告结构化解析
│       └── analyzer.rs  # 内存溢出、Java不匹配、依赖缺失、冲突分析
└── tests/               # 独立测试集合 (与 src 完全分离)
    ├── paths_test.rs        # 路径解析测试
    ├── account_test.rs      # 账户与 UUID 测试
    ├── config_test.rs       # 配置读写测试
    ├── java_test.rs         # Java 输出解析与兼容测试
    ├── version_test.rs      # 版本 JSON 与规则测试
    ├── launch_test.rs       # 启动命令与参数拼装测试
    ├── task_test.rs         # 任务生命周期与文件哈希测试
    ├── downloader_test.rs   # 下载地址转换与版本目录测试
    ├── mod_manager_test.rs  # 模组目录与扫描测试
    └── diag_test.rs         # 日志过滤、崩溃报告解析与原因推断测试
```

---

## 常用命令 (Commands)

```bash
# 代码检查
cargo check

# 运行全套测试 (39 个分离集成测试)
cargo test
```
