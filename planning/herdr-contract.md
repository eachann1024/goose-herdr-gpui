# Herdr 客户端与终端契约记录

记录基线：2026-09-18 对 `/Users/eachann/Work/goose-herdr` 源码的静态核查。以下内容是源码观察记录，不代表目标客户端当前构建或运行验收已完成。

## HerdrKit 连接与会话

- `Packages/HerdrKit/Sources/HerdrKit/Device.swift`：本地连接使用 Unix socket；SSH 通过 OpenSSH stream-local forward；Tailcat 使用 WireGuard 连接。默认 socket 为 `~/.config/herdr/herdr.sock`，命名 session 使用 `~/.config/herdr/sessions/<name>/herdr.sock`。
- `Packages/HerdrKit/Sources/HerdrKit/SocketRPC.swift`：协议采用换行分隔的 JSON-RPC。普通请求使用短连接；事件流使用持久连接并调用 `events.subscribe`，重连由调用方负责。
- `Packages/HerdrKit/Sources/HerdrKit/HerdrService.swift`：按设备管理 tunnel 与 RPC client；连接后先 ping 并检查最低协议版本，再调用 `session.snapshot`、`pane.get/read/send_input/close` 并订阅事件。
- 同文件中的终端 attach 经由 `herdr` CLI 执行：`agent attach <pane_id> --takeover` 或 `terminal attach <terminal_id> --takeover`。`HERDR_SOCKET_PATH` 可覆盖 socket；SSH 与 Tailcat 使用各自连接路径。
- `HerdrSessionDiscovery.swift`：每个命名 session 有独立 server/socket；只枚举 socket 文件存在的 session，陈旧 socket 交由连接错误处理。

## 进程与终端所有权

- `Packages/HerdrKit/Sources/HerdrKit/LocalServer.swift`：本地首次连接时可按需查找并启动 PATH 中的 `herdr server`。daemon 是共享进程，不归 GUI 所有，应用退出时不会将其终止；GUI 环境变量会在启动时清理，避免身份或动态库环境污染 daemon。
- `Sources/GooseAgent/TerminalView.swift`：本地 shell 由客户端 `TerminalProcess` fork/exec PTY；Ghostty host surface 负责渲染，并将键盘、设备属性及 OSC 回复写回 PTY。
- `Sources/GooseAgent/ContentView.swift`：Herdr attach sessions、本地 shell sessions 与 split shells 可同时存在。非当前 attach 保留在视图层级中，避免拆除 NSView/PTY；断开时显示单项重连界面。
- Herdr daemon 管理业务 pane；客户端 PTY 属于本地 shell/终端 attach 客户端路径。实现与验证应维持这两种职责边界，不创建第二个业务 daemon 或绕过 Herdr session ownership。

## UI 与已有能力

- `Sources/GooseAgent/SidebarView.swift`：侧栏管理设备、workspace/space 与 agent session，支持分组、拖拽排序、状态显示和快捷切换。
- `GooseAgentApp.swift` 与 `TerminalSplitTree.swift`：原生菜单支持横向/纵向 split、四向 focus 和 swap；split tree 支持比例调整及邻居导航。
- `HerdrService.swift`：提供 snapshot、workspace/agent catalog、pane 创建/读取/输入/关闭、事件订阅、agent/terminal attach，以及附件和终端命令能力。
- `ContentView.swift`：包含连接失败与重连、Tailcat/VPN 状态、SSH fallback、Keychain token、文件和附件上传，以及终端字体、字号、行距、鼠标报告偏好。

## 尚需重新验证的项目

上述文件行号与版本来自 2026-09-18 的静态观察。若继续开发，应针对当前 HerdrKit 与客户端源码重新确认 daemon 启停、JSON-RPC 错误与重连语义、PTY 所有权、TerminalProcess 行为、尺寸与 IME、认证和附件流程，并以实际构建和运行结果记录验收状态。
