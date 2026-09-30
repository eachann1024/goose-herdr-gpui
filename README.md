# Goose Herdr GPUI

Goose Herdr 的 macOS Rust/GPUI 客户端。终端改用固定版本的 **Zed 终端核心与批量网格渲染代码**（`vendor/zed-terminal`），替换原自研 PTY 读取、输入编码和逐格绘制；Herdr 会话、SSH、分栏及配置由宿主适配。不是原封不动嵌入 Zed 的完整 `terminal_view` crate。来源、修改边界及 GPL-3.0-or-later 许可见 [`vendor/zed-terminal/SOURCES.md`](vendor/zed-terminal/SOURCES.md)。不包含 Goose Hub 的商店和组件宿主（S2）。`planning/` 保留历史方案，其中 TA 原创终端方案已被本次选择取代。

## 视频介绍

https://github.com/user-attachments/assets/4e1b464f-9ab5-42a0-a888-820aeb9f13ba

## 构建

需要 macOS、Rust/Cargo、Xcode Command Line Tools，以及原 Goose Herdr 源码中的 Tailcat 静态制品。依赖版本见 `Cargo.lock`。

```sh
cd /Users/eachann/Work/goose-herdr-gpui
cargo build --locked
```

默认 Tailcat 路径是相邻 `goose-herdr/Packages/HerdrTailcat/Artifacts/Tailcat.xcframework/macos-arm64_x86_64`。其他位置可设置 `HERDR_TAILCAT_FRAMEWORK_DIR`（包含 `Tailcat.framework` 的目录）。构建不启动 Herdr、应用或任何用量 provider。

## macOS 应用包

```sh
cd /Users/eachann/Work/goose-herdr-gpui
scripts/build-app.sh
# 验收：编译 app 并安装到 /Applications/Goose herdr GPUI.app
scripts/build-app.sh --install
# 已完成 debug 构建时，仅打包：
scripts/build-app.sh --debug --no-build
# 保留首包，生成另一份产物：
scripts/build-app.sh --debug --no-build --output dist/repair
```

输出 `/Users/eachann/Work/goose-herdr-gpui/dist/Goose herdr GPUI.app`，独立 bundle id 为 `dev.eachann.goose-herdr-gpui`。默认不覆盖已有产物；重新打包前自行移走，或用 `--install` 覆盖 dist 包并替换 `/Applications/Goose herdr GPUI.app`。若该应用正在运行，`--install` 会先退出再复制。不要结束 Swift 版 Goose Agent。默认 release，可用 `--debug`；`--output` 指定另一输出目录；沿用 `CARGO_HOME`、`CARGO_TARGET_DIR`、`CARGO_BUILD_TARGET` 等环境配置，不依赖特定临时目录。

`HERDR_SOURCE_DIR` 可指定原 Goose Herdr 源码位置（默认相邻目录）。打包会调用其 `scripts/package-usage-helper.sh`，按固定 SHA256 下载/复用 Node runtime，并复制 UsageHelper 和依赖许可；不读取账户凭据。当前 Tailcat 静态链接，不需要打包 framework。若后续出现非系统动态库，脚本会拒绝生成不完整包。

应用包仅做本地 ad-hoc 签名；没有 Developer ID 签名、公证或公开发行许可结论。通用 UI 使用内嵌 Zed SVG，品牌图标沿用原项目；来源、版本和许可见 `resources/icons/SOURCES.md` 与 `LICENSES`，打包时保留声明。

构建后由用户按需运行（以下命令会启动应用并连接配置的设备）：

```sh
open "/Users/eachann/Work/goose-herdr-gpui/dist/repair/Goose Herdr GPUI.app"
```

## Ghostty 功能映射

采用功能语义映射，不读取或覆盖 Ghostty 配置、不切换终端引擎。设置入口为「设置 → 终端」，保存后应用于已有与新建终端。

| 功能 | 本客户端的实现与边界 |
| --- | --- |
| `window-padding-x/y` | 左右/上下内边距，默认各 8 逻辑像素，范围 0–64；本轮只提供对称边距。 |
| `window-padding-balance` | 可选均分不足一格的余量，默认关闭；语义对应等分，不实现 Ghostty 顶部留白上限策略。 |
| 外边距 | Herdr 自有设置，非 Ghostty 同名选项；终端与分栏边缘留白，默认 0，范围 0–64。 |
| `unfocused-split-opacity` | 仅淡化非活动分栏内容，范围 0.15–1；默认 1，保留原有外观，抓手和错误提示不淡化。 |
| `copy-on-select` | 可选写入系统剪贴板，默认关闭；保留选区，普通单击不覆盖剪贴板。 |

现有字体、字号、字重、行距、搜索、分栏和快捷键继续复用，不另建平行实现。透明/模糊窗口、着色器、Ghostty 配置文件导入暂不映射：它们依赖不同的窗口/渲染或配置语义。未接入的光标闪烁、滚动历史上限等也不显示为可用设置。

内边距、整格尺寸、鼠标定位和输入法候选框使用同一内容坐标；极小分栏按终端核心要求保留两列一行并裁剪。字体留空时回退 Menlo，字号和行距仍生效。

语义参考：[Ghostty 官方配置源码](https://github.com/ghostty-org/ghostty/blob/main/src/config/Config.zig)；实现复用本项目锁定的 Zed 核心与 GPUI 0.2.2，不新增依赖。

## 数据与运行边界

- 兼容访问旧 `dev.eachann.goose-herdr` 偏好域与原 Keychain 标识；不是复制数据库或迁移共享 daemon。实际修改设置会写回兼容域，避免同时使用两个客户端编辑同一设置。
- Herdr CLI、daemon、socket 和远端会话保持原协议。GUI attach PTY 与独立 shell PTY 分离；退出 GUI 不表示关闭共享 daemon。
- 用量 helper 仅在运行应用并启用对应 provider 时执行；Cursor 需要显式只读授权。OpenCode 缺少既有会话 cookie 时显示不可用，不生成假数据。
- `thinStrokes` 保留旧偏好但不宣称细笔画效果：旧 renderer 已忽略此值，当前 GPUI 无公开细化接口，设置页明确限制。
- 开发运行可设置 `GOOSE_USAGE_HELPER` 指向完整 UsageHelper（包括对应架构 runtime）；常规 `.app` 从自身 Resources 读取。

## 验收

当前侧栏修正包：`/Users/eachann/Work/goose-herdr-gpui/dist/sidebar-icons/Goose Herdr GPUI.app`。`Space` 标题与右侧箭头、会话卡片留白和图标尺寸已修正。本机项目优先读取自带 Logo，再以 GPUI/Rust、Java、Python、JS/TS、HTML、Pi 插件类型回退，未知项目用文件夹；侧栏、会话和搜索共用缓存，远程路径不会用于本机文件读取。`cargo build --locked` 成功，`cargo test --locked` 36 项通过，应用包 ad-hoc 签名校验通过；日志在 `dist/sidebar-icons/`。未启动新版或做视觉/真实会话交互实测，未覆盖旧包。

上一轮布局对齐包：`/Users/eachann/Work/goose-herdr-gpui/dist/aligned/Goose Herdr GPUI.app`。已对齐标题栏、空间与会话侧栏、设备底栏、设置及错误浮层；补齐菜单、排序、优先会话、快捷键兼容和显式接管提示。`cargo build --locked` 成功，`cargo test --locked` 33 项通过，应用包签名校验通过；日志在 `dist/aligned/`。未启动此包、未连接真实会话或进行视觉/交互实测，旧包未覆盖。应用仍共享原偏好与设备数据，自动恢复不抢占已有客户端。

2026-09-18 修复包：`/Users/eachann/Work/goose-herdr-gpui/dist/repair/Goose Herdr GPUI.app`。`cargo build --offline --locked` 成功，24 项检查通过，arm64 应用包包含 UsageHelper runtime，ad-hoc 签名深度校验通过；日志与哈希见 `dist/repair/build-verification.txt`。

此前 `dist/final` 只通过编译与打包检查，没有运行应用，不能说明界面或核心流程可用；旧包保留，不再作为当前交付包。文字全空白的根因是关闭 GPUI 默认特性后遗漏 `font-kit`，macOS 使用了无字形输出的 `NoopTextSystem`，现已启用真实字体后端。另修复终端高度链、未布局时错误缩为一行、设备/空间视图隔离、实际 socket 复用与明确接管入口；自动恢复不会强制抢占会话。

隔离运行已观察到正文文字、设置和文件界面可见；真实独立 zsh PTY 的 `printf` 输出可见，`Cmd+D` 左右分栏分别输出正常，点击左侧正文后 `Cmd+W` 仅关闭左侧并保留右侧，终端尺寸为 39 行 × 123 列。证据见 `dist/repair-evidence/runtime-check.txt` 与 `terminal-split-ok.png`。真实 Herdr daemon、远端会话、Keychain、通知与用量 provider 未做端到端验收。不要将纯状态检查或隔离本地运行等同于这些外部功能已验证。

本机全局 Cargo 镜像配置干扰构建，本轮统一构建使用隔离的 `CARGO_HOME`（`/tmp/goose-herdr-cargo-home`）从 `/tmp` 执行带 `--manifest-path /Users/eachann/Work/goose-herdr-gpui/Cargo.toml` 的离线构建；这是本机环境规避，不是项目对该临时目录的依赖。正常环境使用上面的标准命令。

旧窗口不会由构建过程退出；打开修复包前先自行退出旧 GPUI 应用，以免 macOS 复用仍运行的旧进程。没有安装覆盖旧版，没有迁移共享 daemon 或用户数据。

## 许可

自有代码以 [MIT 许可证](LICENSE) 开源，版权所有 © 2026 eachann1024。`vendor/` 下第三方代码（含 GPL-3.0-or-later 的 `vendor/zed-terminal`）保持其原许可证，不适用 MIT；详见 [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES)。包含该 GPL 代码的二进制分发须遵守 GPL-3.0-or-later 要求。
