# AGENTS.md — FXLauncher 研发指南

本项目是一个基于 Rust（2024 edition）开发的跨平台轻量化 Minecraft 启动器：核心后端库，加上用 [aegle](https://github.com/antinomie1/aegle) 标记语言编写的桌面界面（`ui` feature，默认开启）。

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
   - 下载优先使用所选源（默认 BMCLAPI），失败时自动回退到另一个源。

6. **界面与后端分离 (UI Separated from Backend)**
   - 界面结构写在 `src/ui/*.aegle` 标记文件中，行为由 Rust 通过 `host.*` 动作实现，数据通过标记中的 state 回推。
   - `src/ui` 只依赖后端公开 API；后端模块不得依赖 `ui`。`--no-default-features` 必须能编译并通过测试。

---

## 项目结构 (Directory Structure)

```text
fxlauncher/
├── AGENTS.md            # 本规范文件
├── Cargo.toml           # 项目定义 (Rust 2024)
├── README.md            # 项目说明与测试指引
├── src/                 # 后端核心源码
│   ├── lib.rs           # 库导出与公共模块暴露
│   ├── main.rs          # 入口：启动桌面界面（需要 ui feature）
│   ├── env/             # 环境探测与平台路径
│   │   ├── paths.rs     # 跨平台路径计算与版本子目录辅助函数
│   │   └── java.rs      # Java 探测、版本提取与版本兼容匹配
│   ├── game/            # 游戏核心模型与运行
│   │   ├── account.rs   # 离线与微软账户模型、UUID 派生、账户列表持久化
│   │   ├── config.rs    # 启动器配置存储与持久化 (JSON)
│   │   ├── version.rs   # version.json 解析、inheritsFrom 合并、规则与类路径装配
│   │   ├── natives.rs   # 本地库 (LWJGL 等) 解压到 versions/<id>/natives
│   │   ├── launch.rs    # 启动命令组装、JVM 参数与进程管理
│   │   ├── loader.rs    # 模组加载器检测 (Fabric, Forge, NeoForge, Quilt, OptiFine)
│   │   ├── mods.rs      # 实例模组管理 (开启/关闭/删除/冲突诊断)
│   │   ├── shaders.rs   # 光影包管理与激活切换 (optionsshaders.txt)
│   │   └── saves.rs     # 地图存档管理 (列表/大小/备份/重命名/删除)
│   ├── net/             # 网络、任务管理与内容下载
│   │   ├── client.rs    # HTTP 请求与参数编码轻量封装
│   │   ├── auth.rs      # 微软正版验证 (设备码 OAuth → Xbox → XSTS → Minecraft)
│   │   ├── profile.rs   # Minecraft 档案：玩家名、皮肤、披风
│   │   ├── task.rs      # 全局非阻塞任务管理与 SHA-1 校验
│   │   ├── batch.rs     # 多线程批量下载 (镜像回退、已存在文件跳过)
│   │   ├── download.rs  # 版本清单、镜像地址映射 (官方 + BMCLAPI)
│   │   ├── install.rs   # 安装流程：核心、依赖库、本地库、资源文件、实例
│   │   ├── fabric.rs    # Fabric / Quilt 安装 (元数据服务器)
│   │   ├── forge.rs     # Forge / NeoForge 安装 (解析安装器并运行处理器)
│   │   └── mods.rs      # 模组搜索与下载管理 (Modrinth + CurseForge)
│   ├── diag/            # 诊断、日志与崩溃分析
│   │   ├── log.rs       # 日志行解析与错误过滤
│   │   ├── crash.rs     # 标准崩溃报告结构化解析
│   │   └── analyzer.rs  # 内存溢出、Java不匹配、依赖缺失、冲突分析
│   └── ui/              # 桌面界面 (ui feature)
│       ├── main.aegle   # 界面结构与 state (启动/实例/下载/任务/设置)
│       ├── parts.aegle  # record 与可复用组件 (任务卡片、模组行等)
│       ├── mod.rs       # 窗口、主题与视图挂载
│       ├── accounts.rs  # 账户页：微软登录与档案管理
│       ├── actions.rs   # host.* 动作注册
│       ├── controller.rs# 启动、任务、设置与后台消息
│       ├── content.rs   # 实例内容管理与下载页
│       └── model.rs     # 后端数据 → 标记 record 的转换
└── tests/               # 独立测试集合 (与 src 完全分离)
    ├── paths_test.rs        # 路径解析测试
    ├── account_test.rs      # 账户、UUID、账户列表、档案解析与皮肤校验测试
    ├── config_test.rs       # 配置读写测试
    ├── java_test.rs         # Java 输出解析与兼容测试
    ├── version_test.rs      # 版本 JSON 与规则测试
    ├── launch_test.rs       # 启动命令与参数拼装测试
    ├── task_test.rs         # 任务生命周期与文件哈希测试
    ├── downloader_test.rs   # 下载地址转换与版本目录测试
    ├── mod_manager_test.rs       # 模组目录与扫描测试
    ├── instance_content_test.rs  # 加载器检测、模组启闭、光影与存档管理测试
    ├── diag_test.rs              # 日志过滤、崩溃报告解析与原因推断测试
    ├── install_test.rs           # 继承合并、特性规则、镜像地址、本地库解压测试
    ├── ui_test.rs                # 无窗口挂载界面并可渲染各页面 PNG
    └── network_test.rs           # 真实网络安装 (默认忽略)
```

---

## 常用命令 (Commands)

```bash
# 代码检查
cargo check

# 运行全套测试 (58 个分离集成测试；Linux 构建界面需要 libxkbcommon 与 fontconfig 开发文件)
cargo test

# 仅后端 (不编译界面)
cargo test --no-default-features

# 运行桌面界面
cargo run --release

# 将各页面渲染为 PNG 以检查界面
FX_UI_SNAPSHOT_DIR=target/ui cargo test --test ui_test

# 真实网络安装测试 (下载约 1 GB)
cargo test --release --test network_test -- --ignored --nocapture
```
