# FXLauncher (Minecraft 启动器核心后端)

FXLauncher 是一个采用 **Rust 2024** 编写的跨平台轻量化 Minecraft 启动器核心后端库。

---

## 模块说明 (Modules)

- **`paths`**：跨平台解析操作系统对应的 `.minecraft` 目录、子目录与类路径分隔符。
- **`config`**：启动器配置管理，支持 JSON 持久化、版本隔离开关与下载源切换。
- **`account`**：离线模式（RFC 4122 v3 UUID 派生）与在线认证账户模型。
- **`java`**：扫描系统 Java 安装路径，解析主版本号并计算版本兼容性。
- **`version`**：解析 Minecraft `version.json`，支持操作系统规则过滤与类路径装配。
- **`launch`**：参数占位符替换、JVM 参数构建与子进程管理。
- **`task`**：全局非阻塞下载任务管理器，支持状态查询、即时网速、进度更新、暂停、恢复、取消及 SHA-1 校验。
- **`downloader`**：支持 Mojang 官方源与 BMCLAPI 镜像源，拉取版本清单、元数据与客户端核心文件。
- **`mod_manager`**：Modrinth / CurseForge 模组搜索与下载，将模组存放于版本专属目录。

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
