# Goose Herdr GPUI

本仓库是 GPUI + Rust 的 macOS 客户端。全局协作、技能路由与「不要自行创建分支」见用户主 `AGENTS.md`。这里只写本仓库会改变决策的规则。

## 主会话与子代理

主会话负责需求对齐、任务调度、关键决策和总结；主会话应先调研清楚风险点并给出完整的解决方案。

激进地使用子代理，越多越好，目的是节省时间。绝大部分情况使用 `low` 模型。界面/样式实现用子代理，模型与当前主会话相同、思考强度 `low`。

## 界面

做任何窗口、侧栏、标题栏、菜单、搜索、设置或动效前，先读 `native-design` 的 GPUI 分支：

- [入口与工作顺序](/Users/eachann/.agents/skills/native-design/SKILL.md)
- [GPUI 指南](/Users/eachann/.agents/skills/native-design/gpui/guide.md)
- 涉及样式时读 [视觉令牌](/Users/eachann/.agents/skills/native-design/gpui/tokens.md)，并从 [截图索引](/Users/eachann/.agents/skills/native-design/gpui/references/README.md) 打开对应图
- API、焦点、图标许可按 [官方来源](/Users/eachann/.agents/skills/native-design/sources.md) 按需查询

不要硬写交互。同类能力先看本仓库已有组件，再对照 **Zed 官方实现**（`crates/gpui`、`crates/ui`、焦点/菜单/搜索/标题栏）和可用的 GPUI 组件库（含 [gpui-kit](https://github.com/longbridge/gpui-kit)）。以本仓库 `Cargo.lock` 锁定版本为准，在线 `main` 只作线索。Zed 部分 crate 可能是 GPL，复制或接入前核对许可；不为模仿参考产品新增业务功能。保留已有功能、权限、本地化和用户改动。

工作台所有右键菜单必须走独立组件 `src/context_menu.rs`。该文件用途是统一浮层、分组、键盘与右侧快捷键列。给右键项加快捷键时：1) 把动作放进 `app_shortcuts::SHORTCUTS`；2) 在 `context_menu.rs` 的 `shortcut_action` 把对应 `UiAction` 映射到 GPUI Action；3) 绑定 Workbench 与 ContextMenu（若菜单打开时根上下文是 ContextMenu）。禁止在各处手写菜单行或只给某一项临时画快捷键。

## 验收

编译成功即通过，不强制启动、截图或交互实测。用户另要求视觉/交互核对时再补充，并在结果里区分「编译通过」和「视觉/交互已验证」。

验收命令是编译 **app**，不是只跑 `cargo check`：

```sh
scripts/build-app.sh --install
```

`--install` 会在编译打包后安装到 `/Applications/Goose herdr GPUI.app`：

1. 若 `goose-herdr-gpui` 或「Goose herdr GPUI」正在运行，先退出再替换。
2. 未运行则直接复制替换。

不要结束 Swift 版 Goose Agent，也不要覆盖 `/Applications` 里其他 Herdr 客户端。
