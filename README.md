# FXLauncher (Minecraft 启动器核心后端)

FXLauncher 是一个采用 **Rust 2024** 编写的跨平台轻量化 Minecraft 启动器核心后端库。

---

## 模块说明 (Modules)

- **`env`**：
  - `paths`：跨平台解析操作系统对应的 `.minecraft` 目录、版本子目录与类路径分隔符。
  - `java`：扫描系统 Java 安装路径，解析主版本号并计算版本兼容性。
- **`game`**：
  - `config`：启动器配置管理，支持 JSON 持久化、版本隔离开关与下载源切换。
  - `account`：离线模式（RFC 4122 v3 UUID 派生）与在线认证账户模型。
  - `version`：解析 Minecraft `version.json`，支持操作系统规则过滤与类路径装配。
  - `launch`：参数占位符替换、JVM 参数构建与子进程管理。
  - `loader`：自动检测实例的模组加载器（Fabric、Forge、NeoForge、Quilt）与 OptiFine。
  - `mods`：实例专属模组列表、开启/关闭切换（`.disabled`）、删除与兼容性/冲突分析。
  - `shaders`：光影包列表、当前激活检测、激活切换（`optionsshaders.txt`）与删除。
  - `saves`：世界地图存档列表、计算大小、备份复制、重命名与删除。
- **`net`**：
  - `client`：HTTP GET 请求、JSON 提取与参数编码统一轻量封装。
  - `task`：全局非阻塞下载任务管理器，支持状态查询、即时网速、进度更新、暂停、恢复、取消及 SHA-1 校验。
  - `download`：支持 Mojang 官方源与 BMCLAPI 镜像源，拉取版本清单、元数据与客户端核心文件。
  - `mods`：Modrinth / CurseForge 模组搜索与下载，将模组存放于版本专属目录。
- **`diag`**：
  - `log`：结构化日志行解析与错误行提取。
  - `crash`：标准 Minecraft 崩溃报告解析（Description、Suspected Mods、System Details）。
  - `analyzer`：智能原因推断（内存溢出、Java 版本不匹配、模组依赖缺失、模组冲突、显卡驱动异常）与排查建议生成。

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

---

## 开源协议

本项目采用 MIT 或 Apache-2.0 双重许可证。
