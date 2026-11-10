# Goose Herdr GPUI

GPUI + Rust 的 macOS 客户端。

## 子代理

主会话负责需求对齐、风险调研、方案和总结。相互独立、可并行的实现分给子代理以节省时间，默认思考强度 `low`；界面/样式实现的子代理用与主会话相同的模型。

## 界面

- 不硬写交互：先看本仓库已有组件，再对照 Zed 官方实现（`crates/gpui`、`crates/ui`、焦点/菜单/搜索/标题栏）和 GPUI 组件库（如 [gpui-kit](https://github.com/longbridge/gpui-kit)）。版本以本仓库 `Cargo.lock` 为准，在线 `main` 只作线索。
- Zed 部分 crate 可能是 GPL，复制或接入前核对许可。不为模仿参考产品新增业务功能；保留已有功能、权限、本地化和用户改动。
- 右键菜单统一走 `src/context_menu.rs`（浮层、分组、键盘与右侧快捷键列），不在别处手写菜单行或临时画快捷键。给右键项加快捷键：1) 动作放进 `app_shortcuts::SHORTCUTS`；2) 在 `context_menu.rs` 的 `shortcut_action` 把 `UiAction` 映射到 GPUI Action；3) 绑定 Workbench 与 ContextMenu（若菜单打开时根上下文是 ContextMenu）。

## 验收

- 运行 `scripts/build-app.sh --install`（不是只跑 `cargo check`）：编译打包并安装到 `/Applications/Goose herdr GPUI.app`，应用在运行时会先退出再替换。编译成功即通过。
- 用户要求视觉/交互核对时再补做，并在结果里区分「编译通过」和「视觉/交互已验证」。
- 不结束 Swift 版 Goose Agent，不覆盖 `/Applications` 里其他 Herdr 客户端。

## 提交与推送

- 改动验收后直接提交并推送 `main`，无需询问：先保留未提交工作，`git fetch origin` 并合入 `origin/main`，逐项解决冲突（保留双方有效改动），再只原子提交本任务改动，提交说明用英文。
- 推送被拒就重新同步、复核后重试，直到远端包含本次提交；不强制推送，不覆盖无关工作。当前不在 `main` 时先告知用户。
