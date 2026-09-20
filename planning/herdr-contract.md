# Herdr / 终端底层契约调研（2026-09-18）

## 范围与结论

- `/Users/eachann/Work/goose-herdr-gpui` 当前是初始无业务工程：已有其他代理写入的 `planning/` 调研文件，但尚无 Zed/GPUI 业务源码、Git 元数据或 AGENTS.md；不能据此确认接入、版本、进程或迁移进度。
- `/Users/eachann/Work/goose-hub` 当前工作树有大量未提交改动；本调研只读，未启动服务、未执行 `herdr`，也未设置 `HERDR_ENV`。
- Goose Hub 的确有独立 Goose Run/libghostty 终端，但非 Herdr。真实 Herdr 工程为 `/Users/eachann/Work/goose-herdr`，不是目标目录；其 HerdrKit 契约见下文。
- 可确认的现行终端契约是 Goose Run 的 **libghostty 原生 Metal NSView + 进程内 PTY**，不是 Herdr API：迁移到 Zed/GPUI 时建议把此终端/PTY/会话/生命周期边界作为保留层，GPUI 只替换 UI/宿主承载。

## 已确认契约（源码证据）

### 技术栈与边界

- `/Users/eachann/Work/goose-hub/components/run/DESIGN.md:11-22`：产品壳 Tauri 2；终端为进程内 libghostty Metal NSView，WebView 仅报告 bounds/Run 注入，默认不挂载；脚本库 Rust 读写 `scripts.json`；历史 Electron/GPUI/xterm.js+tauri-pty 已退役。
- `/Users/eachann/Work/goose-hub/components/run/src/platform.ts:4-20`：传输可走 `window.goose.component.invoke` 或 Tauri invoke，但命令名与 native terminal behavior 应保持不变。

### 会话、pane 与 attach/重连语义

- `/Users/eachann/Work/goose-hub/components/run/src/terminal.ts:1-3,18-51`：一 `scriptId` 对一 session，另有 `free`；会话状态 idle/running/stopped；保存 PID、shell、home、generation、异步 PTY 操作、注入定时器和 resize observer；最多 8 个 live sessions（37-40）。
- `/Users/eachann/Work/goose-hub/components/run/src/terminal.ts:418-513`：`ensureSession` 对同 id 复用并重新 attach host；新会话原生 `term_ensure`，随后主题、字体、frame；默认可自动 spawn。不存在独立 Herdr attach/reconnect 协议；所谓重新挂载是 renderer session 复用 + native surface attach。
- `/Users/eachann/Work/goose-hub/components/run/src/terminal.ts:515-541`：激活会话时只显示目标 native surface、隐藏其他 pane，并 focus/blur；`intendedVisible` 用于 occlusion 恢复。
- `/Users/eachann/Work/goose-hub/components/run/src/terminal.ts:630-676`：dispose 删除 session、异步 `term_dispose`；宿主 stop handshake 会先退休 spawn、等待 native disposal；可由宿主接管 native cleanup（`skipNative`）。

### PTY、进程、shell、env 与注入

- `/Users/eachann/Work/goose-hub/components/run/main/index.ts:24-38`：原生命令 schema；session id/text 禁 NUL，注入文本上限 2,000,000 字符，font 8..36，frame 为有限数；命令包括 ensure/spawn/inject/focus/visibility/frame/theme/occlusion/pid/dispose。
- `/Users/eachann/Work/goose-hub/components/run/main/index.ts:92-116`：运行时创建临时 `zsh-login` wrapper，执行 `/bin/zsh -l -o NO_BANG_HIST`；shell 解析顺序为 dscl 用户 shell → `$SHELL` → `/bin/zsh`。
- `/Users/eachann/Work/goose-hub/components/run/main/index.ts:184-213`：`pids_busy` 只允许检查本运行时拥有的 PID；spawn 默认 cwd 为 home，env 由 schema 传入；会话数硬上限 8；非 bound 时 pid 返回 0、其他原生调用返回 null。
- `/Users/eachann/Work/goose-hub/components/run/src/terminal.ts:552-610`：运行脚本先重建 shell，再延迟 200ms 注入（800ms fallback）；zsh 必须设置 `NO_BANG_HIST`；脚本文本在用户交互 zsh 中原文执行，保留用户 rc（`script-inject.ts:1-15`）。
- `/Users/eachann/Work/goose-hub/components/run/crates/goose-run/src/terminal_host.rs:11-34,143-189`：Rust FFI 对应上述原生命令，spawn 参数包括 id/cwd/shell/env；所有 Ghostty 操作切回主线程。

### 尺寸、渲染与原生资源

- `/Users/eachann/Work/goose-hub/components/run/src/terminal.ts:224-244,267-284`：以 DOM `getBoundingClientRect` 上报 x/y/width/height；ResizeObserver + RAF 更新；原生层接收 frame。
- `/Users/eachann/Work/goose-hub/components/run/crates/goose-run/src/terminal_host.rs:232-249`：frame 是 f64 坐标；由 native `gr_term_set_frame` 设置。
- `/Users/eachann/Work/goose-hub/components/run/main/index.ts:125-152`：cleanup 先移除 handler、调用 addon.shutdown（Ghostty surface 销毁会关闭其拥有的 PTY、observer、input state），再清 wrapper；dispose 最终关闭脚本库。

### 持久化与信任边界

- `/Users/eachann/Work/goose-hub/components/run/main/index.ts:40,70-74,154-181,223-246`：Rust library methods 管理 `scripts.json`/`window.json`；输入由 Zod schema 校验；路径必须绝对且可访问后才允许 reveal；数据校验不创建 PTY/初始化 Ghostty。
- `/Users/eachann/Work/goose-hub/components/run/components/SettingsPanel.tsx:169-208`（当前工作树）：产品说明明确“空 shell 与运行会话共用同一 PTY 面”“真实 PTY”“不提供 Agent/HTTP 控制/运行历史”；设置页标注 libghostty。


### 真实 HerdrKit 契约（/Users/eachann/Work/goose-herdr）

- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Package.swift:1-25`：Swift Package `HerdrKit`，macOS 14/iOS 18，依赖本地 `HerdrTailcat`，Swift 5 language mode。
- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/Device.swift:1-18,50-55`：local 直连 Unix socket；SSH 通过 OpenSSH stream-local forward；Tailcat 通过 herdr.tailcat；默认 socket `~/.config/herdr/herdr.sock`，named session 使用 `~/.config/herdr/sessions/<name>/herdr.sock`。
- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/SocketRPC.swift:1-4,38-40,134-160`：换行分隔 JSON-RPC；普通请求每次新连接；事件流持久连接并发送 `events.subscribe`；断线由调用者负责重连。
- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/HerdrService.swift:4-12,36-71,140-145,534-576,682-714`：按设备管理 tunnel/client；先 ping 且校验最低 protocol version；调用 `session.snapshot`、`pane.get/read/send_input/close`，并订阅事件。
- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/HerdrService.swift:717-854`：terminal attach 通过 herdr CLI；`agent attach <pane_id> --takeover` 或 `terminal attach <terminal_id> --takeover`；`HERDR_SOCKET_PATH` 是 socket override，SSH/Tailcat 路径不同。
- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/HerdrSessionDiscovery.swift:1-9,40-61`：每个 named session 是独立 server/socket；只枚举存在 socket 文件的 session，陈旧 socket 交由正常连接错误处理。
- `/Users/eachann/Work/goose-herdr/Sources/GooseAgent/ContentView.swift:580-589,696-737,758-814`：attach 有 ended overlay、重试计数与显式 reconnect；保持 attach view 在层级中避免拆除 NSView/PTY。

**G0 接入门禁**：在用户确认真实 Herdr 位置/版本前，只能保留上述契约核实；不可把 Goose Hub 的独立 libghostty PTY、Alacritty 或其他 PTY 实现当作已确定 Herdr 替换方案。

## Herdr 相关未确认项（迁移前必须从真实 Herdr 源码/锁定依赖补齐）

1. Herdr upstream/fork 的绝对路径、remote、commit/tag/version、许可证与 ABI。
2. Herdr 进程模型：是否单独 daemon、启动参数、父子关系、PID ownership、退出/重启/升级策略。
3. socket/IPC endpoint、握手、消息 framing、事件顺序、错误码、超时、backpressure、断线与重连/attach 语义。
4. Herdr pane/session 标识是否与当前 `scriptId`/`free`/最多 8 会话一一对应；持久化文件与迁移策略。
5. Herdr 的 shell integration、环境继承、cwd、TERM、窗口尺寸（DIP/backing scale）、焦点/可见性、鼠标/IME/剪贴板协议。
6. 安全边界：可执行文件来源、环境变量/命令注入、跨窗口或跨用户 session 访问控制。

## Zed/GPUI 推荐保留边界

- **保留不变**：PTY 与 shell 启动/注入语义、session id/`free`、8 会话上限、PID ownership、尺寸 frame、焦点/可见性、stop/dispose 顺序、`scripts.json`/`window.json` 兼容、输入校验与“只检查自有 PID”。
- **可替换**：WebView/React/Tauri transport、DOM bounds 采集、NSView 挂载胶水；GPUI 应提供等价的 frame/scale、focus/occlusion、lifecycle callbacks。
- **不要假设**：Herdr 是当前 Goose Run 的实现；现有源码证据只证明 libghostty 原生 PTY。未补齐 Herdr 契约前，不应改动 Herdr 引用、升级版本或设计新的 IPC。

## 2026-09-18 补充：实际 Goose Herdr 依赖、能力与候选 scope

### 依赖与 daemon/sidecar

- `/Users/eachann/Work/goose-herdr/project.yml:18-24,42-45`：macOS/iOS 依赖本地 `HerdrKit`、`HerdrSSH`、`HerdrTailcat`；Ghostty 来自 `https://github.com/Lakr233/libghostty-spm`，精确版本 `1.6.20260909`。
- `/Users/eachann/Work/goose-herdr/GooseHerdr.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved:3-22`：Ghostty revision `7e45d27160f9b34aca9ca5c9820e9207482f9f04`；MSDisplayLink `2.2.0` revision `87eb0af130744c8cbe2e31b6e1a5bcd659f1c220`。
- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/LocalServer.swift:4-12,52-79,119-163`：本机首次连接可按需查找并 spawn `herdr server`；daemon 被刻意视为共享且不归 app 所有，不随 app 退出终止；输出写临时日志。未发现内置 daemon/sidecar 二进制，实际依赖 PATH 中的 `herdr` CLI。
- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/LocalServer.swift:165-198`：启动 daemon 会清理 GUI 身份/DYLD 环境，补齐 SHELL，避免 daemon 继承 App/Xcode 身份污染其所有 pane。

### 客户端语言与终端渲染

- `/Users/eachann/Work/goose-herdr/project.yml:1-16,25-41`：主 app 是 macOS SwiftUI/AppKit（Swift 5.10，macOS 14）；另有 iOS SwiftUI 客户端（iOS 18）。
- `/Users/eachann/Work/goose-herdr/Sources/GooseAgent/TerminalView.swift:270-305`：本地 shell 由自有 `TerminalProcess` fork/exec PTY；`InMemoryTerminalSession` 负责字节写入、网格尺寸同步，Ghostty host surface 渲染；键盘/DA/OSC 回复原样写回 PTY。
- `/Users/eachann/Work/goose-herdr/Sources/GooseAgent/ContentView.swift:592-622,696-750`：终端为 SwiftUI `TerminalSplitLayout`，同时承载 Herdr attach sessions、本地 shell sessions、split shells；非当前 attach 保持在视图层级但 opacity 0，避免拆掉 NSView/PTY；Ghostty surface 使用不透明背景避免离屏合成字形变浅。
- `/Users/eachann/Work/goose-herdr/Sources/GooseAgent/ContentView.swift:573-589,752-790`：字体、字号、行距、鼠标报告等偏好持久化；attach 断开显示 per-entry reconnect overlay，正常 Ctrl-D/远端退出则移除 pane。

### Goose Herdr 完整业务/布局边界

- `/Users/eachann/Work/goose-herdr/Sources/GooseAgent/SidebarView.swift:48-220,552-670,1129-1210`：左侧 sidebar 管理设备、workspace/space、agent session；支持分组、拖拽排序、优先级/状态显示和快捷切换。
- `/Users/eachann/Work/goose-herdr/Sources/GooseAgent/GooseAgentApp.swift:130-170`：原生菜单支持垂直/水平 split、四向 focus、四向 swap；`TerminalSplitTree.swift:1-151` 实现递归 split tree、比例调整、邻居导航。
- `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/HerdrService.swift:140-145,299-333,534-682,713-854`：能力包括 snapshot、workspace/agent catalog、pane 创建/读取/输入/关闭、事件订阅、agent/terminal attach、附件能力与 terminal command 生成。
- `/Users/eachann/Work/goose-herdr/Sources/GooseAgent/ContentView.swift:792-818,850-906`：连接失败/重连、Tailcat WireGuard 无 VPN 连接、SSH fallback、Keychain token、文件/附件上传均属产品能力；不能因迁移终端而删除。

### Scope 对齐候选（不替用户定案）

- **S1（推荐候选）**：迁移现有 `goose-herdr` 的 SwiftUI/AppKit 业务与 HerdrKit 契约；借鉴 Goose Hub 的壳架构/插件生命周期/资源清理，不强行并入 Hub 的 2FA、插件商店等无关业务。优点是“Herdr 业务保持不变”边界清晰，风险最低。
- **S2**：把 Goose Hub 全部业务（插件/组件/2FA/Run 等）与 Herdr 终端、HerdrKit 合并进新 Zed/GPUI 应用。该范围会同时改变产品域与信任边界，必须另立迁移清单，不能默认包含。

当前应停在 G0：等待用户确认真实 Herdr 版本/构建入口及 S1/S2 scope；不得把独立 Alacritty/PT​​Y 视为已批准替换。
