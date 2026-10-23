# FXLauncher (Minecraft 启动器核心后端)

FXLauncher 是一个采用 **Rust 2024** 编写的跨平台轻量化 Minecraft 启动器：核心后端库，加上用 [aegle](https://github.com/antinomie1/aegle) 标记语言编写的桌面界面。空闲时不占用 CPU，常驻内存约 45 MB。

---

## 模块说明 (Modules)

- **`env`**：
  - `paths`：跨平台解析操作系统对应的 `.minecraft` 目录、版本子目录与类路径分隔符。
  - `java`：扫描系统 Java 安装路径，解析主版本号并计算版本兼容性。
- **`game`**：
  - `config`：启动器配置管理，支持 JSON 持久化、版本隔离开关与下载源切换。
  - `account`：离线账户（RFC 4122 v3 UUID 派生）与微软账户模型，以及保存在 `.minecraft/fxlauncher_accounts.json` 的账户列表（Unix 上仅本人可读）。
  - `version`：解析 Minecraft `version.json`，合并 `inheritsFrom` 继承链（Fabric/Forge 配置），按操作系统、架构与启动器特性过滤规则，装配类路径。
  - `natives`：将 LWJGL 等本地库解压到 `versions/<id>/natives`（兼容旧版 `natives` 分类与 1.19+ 的 `natives-*` 构件）。
  - `launch`：参数占位符替换、JVM 参数构建与子进程管理。
  - `loader`：自动检测实例的模组加载器（Fabric、Forge、NeoForge、Quilt）与 OptiFine。
  - `mods`：实例专属模组列表、开启/关闭切换（`.disabled`）、删除与兼容性/冲突分析。
  - `shaders`：光影包列表、当前激活检测、激活切换（`optionsshaders.txt`）与删除。
  - `saves`：世界地图存档列表、计算大小、备份复制、重命名与删除。
- **`net`**：
  - `client`：HTTP GET 请求、JSON 提取与参数编码统一轻量封装。
  - `auth`：微软账户正版验证：设备码 OAuth → Xbox Live → XSTS → Minecraft 服务，校验游戏所有权，启动前自动刷新令牌。
  - `profile`：Minecraft 档案管理：读取档案、修改玩家名、上传/重置皮肤与切换模型、切换披风。
  - `task`：全局非阻塞任务管理器，支持阶段、文件数、即时网速、暂停、恢复、取消及 SHA-1 校验。
  - `batch`：多线程批量下载；已存在且校验通过的文件自动跳过，所选源失败时回退到另一个源。
  - `download`：Mojang 官方源与 BMCLAPI 镜像源的地址映射（含 Forge、NeoForge、Fabric、Quilt 的 Maven 与元数据服务器），版本清单获取。
  - `install`：完整安装流程：客户端核心、依赖库、本地库、资源索引与资源文件（含 1.6 以前的旧版资源布局），以及“原版 + 加载器”的实例安装。
  - `fabric`：通过元数据服务器安装 Fabric / Quilt。
  - `forge`：解析官方安装器安装 Forge / NeoForge：解压 Maven 文件、下载依赖并用 Java 运行客户端安装处理器；兼容 1.12.2 及更早的旧版安装器。
  - `mods`：Modrinth / CurseForge 模组搜索与下载，将模组存放于版本专属目录。
- **`diag`**：
  - `log`：结构化日志行解析与错误行提取。
  - `crash`：标准 Minecraft 崩溃报告解析（Description、Suspected Mods、System Details）。
  - `analyzer`：智能原因推断（内存溢出、Java 版本不匹配、模组依赖缺失、模组冲突、显卡驱动异常）与排查建议生成。

---

## 桌面界面 (UI)

界面结构写在 `src/ui/main.aegle` 与 `src/ui/parts.aegle` 中，Rust 通过 `host.*` 动作实现行为：

- **启动**：当前实例展示、账户与实例切换、启动按钮；自动选择满足版本要求的 Java；游戏异常退出时显示崩溃诊断。
- **账户**：微软账户登录（设备码，自动打开浏览器并填入代码）与离线账户；正版账户显示皮肤正面预览，可修改玩家名、选择并上传皮肤、切换披风。
- **实例**：本地实例列表；模组开关与删除、光影切换、存档备份与删除、崩溃分析。
- **下载**：选择游戏版本与加载器（原版 / Fabric / Quilt / Forge / NeoForge）安装实例；在 Modrinth 上搜索并安装模组。
- **任务**：下载与安装进度，支持暂停、继续与取消。
- **设置**：左侧分类导航（游戏 / Java 与内存 / 下载 / 外观），右侧是该分类的选项：游戏目录、版本隔离、窗口尺寸、Java、内存、JVM 参数、下载源与深浅色主题，修改后自动保存到 `.minecraft/fxlauncher.json`。

Linux 构建需要 libxkbcommon 与 fontconfig 的开发文件；目前 aegle 支持 Wayland 与 Windows。只需要后端库时可以关闭界面：`--no-default-features`。

## 快速开始 (Quick Start)

### 编译检查

```bash
cargo check
```

### 运行启动器

```bash
cargo run --release
```

### 运行全套测试

```bash
cargo test
```

真实网络安装测试默认忽略（会下载约 1 GB）：

```bash
cargo test --release --test network_test -- --ignored --nocapture
```

---

## 开源协议

本项目采用 MIT 或 Apache-2.0 双重许可证。
