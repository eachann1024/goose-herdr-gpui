# Goose Herdr GPUI

Goose Herdr 的 macOS Rust/GPUI 客户端。终端使用固定版本的 Zed 终端核心与批量网格渲染（`vendor/zed-terminal`），Herdr 会话、SSH、分栏和配置由本客户端适配。来源、修改范围及 GPL-3.0-or-later 许可见 [`vendor/zed-terminal/SOURCES.md`](vendor/zed-terminal/SOURCES.md)。

## 视频介绍

https://github.com/user-attachments/assets/0e9bd3ab-c086-4d86-8d15-3282e2ee5866

界面与主要操作的演示。目标平台是 macOS；视频只展示界面，不代表各项功能都已完成端到端验收。

## 构建

需要 macOS、Rust/Cargo、Xcode Command Line Tools，以及 Goose Herdr 源码中的 Tailcat 静态制品。依赖版本见 `Cargo.lock`。

```sh
cargo build --locked
```

默认 Tailcat 路径是相邻目录 `goose-herdr/Packages/HerdrTailcat/Artifacts/Tailcat.xcframework/macos-arm64_x86_64`。其他位置可设置 `HERDR_TAILCAT_FRAMEWORK_DIR`（包含 `Tailcat.framework` 的目录）。构建不会启动 Herdr、应用或用量服务。

## macOS 应用包

```sh
scripts/build-app.sh
# 编译并安装到 /Applications
scripts/build-app.sh --install
# 已完成 debug 构建时，仅打包
scripts/build-app.sh --debug --no-build
# 输出到指定目录
scripts/build-app.sh --output dist/custom
```

产物为 `dist/Goose herdr GPUI.app`，bundle id 为 `dev.eachann.goose-herdr-gpui`。默认不覆盖已有产物；重新打包前先移走，或用 `--install` 覆盖 dist 产物并替换 `/Applications` 中的同名应用。应用正在运行时，`--install` 会先退出再复制。默认 release，可用 `--debug`。脚本沿用 `CARGO_HOME`、`CARGO_TARGET_DIR`、`CARGO_BUILD_TARGET`。

`HERDR_SOURCE_DIR` 指定 Goose Herdr 源码位置（默认相邻目录 `goose-herdr`）。打包会调用其中的 `scripts/package-usage-helper.sh`，按固定 SHA256 下载或复用 Node runtime，并复制 UsageHelper 和依赖许可；不读取账户凭据。当前 Tailcat 为静态链接，不需要打包 framework。若出现非系统动态库，脚本会拒绝生成不完整包。

应用包只做本地 ad-hoc 签名，没有 Developer ID 签名或公证。通用图标使用内嵌 Zed SVG，品牌图标沿用原项目；来源和许可见 `resources/icons/SOURCES.md` 与 `LICENSES`。

构建后按需运行：

```sh
open "dist/Goose herdr GPUI.app"
```

打开应用会连接已配置的设备。

## 终端设置

设置入口为「设置 → 终端」。下列选项按功能语义实现，不读取或覆盖 Ghostty 配置，也不切换终端引擎。保存后对已有终端和新建终端生效。

| 功能 | 实现 |
| --- | --- |
| `window-padding-x/y` | 左右、上下内边距，默认各 8 逻辑像素，范围 0–64，两侧对称。 |
| `window-padding-balance` | 均分不足一格的余量，默认关闭。 |
| 外边距 | 本客户端自有选项：终端与分栏边缘留白，默认 0，范围 0–64。 |
| `unfocused-split-opacity` | 淡化非活动分栏内容，范围 0.15–1，默认 1；分隔条和错误提示不淡化。 |
| `copy-on-select` | 选中后写入系统剪贴板，默认关闭；普通单击不覆盖剪贴板。 |

字体、字号、字重、行距、搜索、分栏和快捷键沿用现有设置。透明或模糊窗口、着色器、Ghostty 配置导入、光标闪烁和滚动历史上限暂不提供。

内边距、单元格尺寸、鼠标定位和输入法候选框使用同一内容坐标。极小分栏至少保留两列一行并裁剪。字体留空时回退 Menlo。

语义参考 [Ghostty 配置](https://github.com/ghostty-org/ghostty/blob/main/src/config/Config.zig)。实现使用本仓库锁定的 Zed 终端核心与 GPUI 0.2.2。

## 数据与运行

- 读写旧偏好域 `dev.eachann.goose-herdr` 和原 Keychain 标识，不复制数据库，也不迁移共享 daemon。修改设置会写回该偏好域，避免两个客户端同时编辑。
- Herdr CLI、daemon、socket 和远端会话保持原协议。界面附着的 PTY 与独立 shell PTY 分开；退出界面不会关闭共享 daemon。
- 用量 helper 只在应用运行且启用对应 provider 时执行。Cursor 需要显式只读授权；OpenCode 没有既有会话 cookie 时显示不可用，不生成占位数据。
- `thinStrokes` 会保留旧偏好值，但当前渲染没有细笔画效果，设置页会说明这一限制。
- 开发运行可设置 `GOOSE_USAGE_HELPER` 指向完整 UsageHelper（含对应架构的 runtime）。打包后的应用从自身 Resources 读取。

## 许可

自有代码以 [MIT 许可证](LICENSE) 开源，版权所有 © 2026 eachann1024。`vendor/` 中的第三方代码（含 GPL-3.0-or-later 的 `vendor/zed-terminal`）保持原许可证，不适用 MIT；详见 [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES)。包含该 GPL 代码的二进制分发须遵守 GPL-3.0-or-later。
