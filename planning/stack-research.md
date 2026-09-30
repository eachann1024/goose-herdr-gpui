# Rust + GPUI 终端技术研究记录

观察基线：2026-09-18。本文比较 GPUI 客户端使用终端渲染代码的实现边界，来源链接均指向上游公开代码。

## 终端组件边界

Zed 的 `terminal_view` 包含 workspace、project、editor、settings、菜单和任务等内部依赖，不是轻量、稳定的独立终端控件。Zed `terminal` 则使用 `alacritty_terminal`、GPUI、vte、libc 及多个 workspace crate。引入或抽取相关代码时，应核对依赖图、固定具体修订并遵守各自许可证。

当前仓库通过 `vendor/zed-terminal/` 保存所采用代码的来源、固定修订、修改范围和许可证说明；以该目录中的 `SOURCES.md` 及许可文件为准。终端解析、网格绘制、焦点、选择、滚动、IME 和 PTY/会话所有权属于不同层，适配时应保持清晰边界。

## Herdr 接入边界

HerdrKit 的 daemon、session 与 pane 契约以 [`herdr-contract.md`](herdr-contract.md) 的源码记录为准。GPUI 视图负责呈现现有会话并传递用户输入、尺寸和焦点变化；连接、认证、事件订阅、attach/reconnect 与业务 pane 生命周期应继续遵守 Herdr 协议。不得在 UI 层复制业务状态或创建第二个 daemon。

本地 shell 与远程 Herdr pane 是不同路径：本地终端可以由客户端 PTY 承载，远程业务 pane 由 Herdr daemon 管理。具体读写、resize、结束与错误处理行为要通过当前 HerdrKit 和客户端源码核实。

## 上游研究来源

- Zed terminal_view manifest 与实现：
  - https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal_view/Cargo.toml
  - https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal_view/src/terminal_view.rs
- Zed terminal manifest 与实现：
  - https://github.com/zed-industries/zed/blob/main/crates/terminal/Cargo.toml
  - https://github.com/zed-industries/zed/blob/main/crates/terminal/src/terminal.rs
- GPUI Kit：
  - https://github.com/longbridge/gpui-kit
  - https://longbridge.github.io/

公开上游会变化；链接用于说明研究出处，不代表当前版本、许可证或 API 已重新核实。发行前以 vendored source、锁定依赖和对应许可证文件为准。
