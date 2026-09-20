# goose-hub → Rust + GPUI + Zed 风格终端技术调研

- 查阅日期：2026-09-18（Asia/Shanghai）
- 范围：只调研，不修改源码、不建分支、不上传私有源码/资产。验收约束为后续实现能编译。
- 结论先行：**主推“借 Zed 的设计与渲染思路，直接使用独立的 alacritty_terminal + 自己的 PTY/GPUI 适配层”**。不要把 Zed `terminal_view` 当可直接嵌入组件；不要 fork 整个 Zed。

## 方案边界（必须区分）

### A（主推）：借 Zed 设计/渲染组件，独立实现
- GPUI 负责窗口、布局、绘制、焦点/输入；终端状态采用 `alacritty_terminal`；**不得默认自有 PTY**：`planning/herdr-contract.md` 与 `planning/capability-audit.md` 显示现有 Run 已由 libghostty/native addon 拥有进程内 PTY；若真实 Herdr 也拥有 PTY/session，则再 spawn shell 会造成双所有者。GPUI 适配层应先接入 Herdr/现有 session 的 attach、读写、resize、生命周期；只有源码确认 Herdr 不拥有 PTY 时，才实现独立 PTY 后端。
- 参考 Zed 的 `TerminalElement`、滚动、选择、IME、焦点、鼠标协议与终端动作，但只移植行为契约和最小代码，不复制 workspace/editor/project 等整套依赖。
- 优点：保留 goose-hub 的 Herdr 底层引用和业务功能；依赖面可控；许可证更清晰；可独立 pin。
- 代价：终端绘制与输入桥接仍需自己完成，不能声称“复用 Zed terminal_view”。

### B：直接嵌 Zed terminal crate（不推荐）
- Zed 的 `terminal` crate 明确依赖 `alacritty_terminal`、`gpui`、`settings`、`theme`、`task` 等内部 workspace crate；`terminal_view` 更进一步依赖 editor、project、workspace、db、menu 等。其 Cargo manifest 还标注 `GPL-3.0-or-later`。这不是一个稳定的独立 terminal widget API，跨仓库嵌入会牵引大量内部 crate 与版本锁定。证据：Zed terminal manifest 与 terminal_view manifest。 
- 若仅为获得 terminal 逻辑而引入，实际会得到“Zed workspace 子集的维护负担”，不符合保持 goose-hub 底层不变的目标。

### C：fork Zed（不推荐，除非接受长期上游合并）
- 可最快得到完整终端功能，但等于承担 Zed 全仓 GPL 许可、庞大内部 API、上游同步和发布合规；还会把 goose-hub 业务与 Zed workspace 强耦合。只有目标变成“维护一个 Zed 分支”时才合理，当前目标不是。

## 证据与真实边界

1. **Zed terminal_view 不是轻量组件**：源文件包含 workspace/project/editor/settings、TerminalPanel、上下文菜单、序列化、任务重跑、搜索、滚动条、路径悬停等；TerminalView 状态本身持有 `Entity<Terminal>`、workspace/project、IME 状态、focus、subscriptions 等。它展示了完整行为清单，但不适合作为外部 API。来源：
   - https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal_view/Cargo.toml
   - https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal_view/src/terminal_view.rs
2. **Zed terminal 底层确实使用 alacritty_terminal**，同时还依赖 `libc`、`vte`、`gpui`、`sysinfo`、`url` 等，说明终端状态解析与 PTY/进程/渲染是分层但非单 crate 即用。来源：
   - https://github.com/zed-industries/zed/blob/main/crates/terminal/Cargo.toml
   - https://github.com/zed-industries/zed/blob/main/crates/terminal/src/terminal.rs
3. **许可证风险**：当前上述 Zed `terminal` 与 `terminal_view` manifest 标为 GPL-3.0-or-later；若复制/链接其代码或分发衍生物，需按 GPL 履行源码、许可与衍生作品义务。不能把“使用 GPUI”与“使用 Zed terminal_view”混为一谈；逐项核对仓库 LICENSE、依赖 license 和再分发方式，最终由法务确认。来源同上 manifest 与 Zed 仓库：https://github.com/zed-industries/zed
4. **GPUI Component 已改名/重定向**：原 `longbridge/gpui-component` 页面重定向至 `longbridge/gpui-kit`；其公开站点声明 Apache-2.0。应 pin Git commit/tag，不依赖浮动 main，并确认实际 Cargo 包和许可证文件。来源：
   - https://github.com/longbridge/gpui-component
   - https://github.com/longbridge/gpui-kit
   - https://longbridge.github.io/

## 推荐依赖与 pin 策略

- GPUI：跟随 goose-herdr-gpui 当前可编译的 commit；不要单独追最新。GPUI API 变化快，`gpui` 与 `gpui-component` 必须同一兼容窗口。
- `alacritty_terminal`：优先使用 Zed 当前锁定/已验证的版本或 commit，记录 Cargo.lock；不要直接引用 Zed workspace path。
- PTY：抽象为本项目自己的极薄接口（spawn/read/write/resize/kill/exit），Unix 先实现 macOS/Linux；Windows 采用 ConPTY 后端。PTY 不负责终端解析、绘制或业务状态。
- 许可证：生成依赖清单（cargo metadata + license 审核），保留 LICENSE/NOTICE；尤其区分 MIT/Apache 依赖和 GPL 的 Zed crate，禁止把 GPL 代码误放入预期许可证的分发物。

## 功能完整性清单（以 capability-audit 为准）

不能用泛型终端清单替代现有能力审计。`planning/capability-audit.md`（扫描日 2026-09-18）要求保持：脚本库 CRUD、分组/搜索、运行/停止、cwd/env/shell/确认运行、真实 PTY、xterm/libghostty/native resize/粘贴/IME、多会话、`window.json`；同时保留 Hub 的组件 catalog/install/enable/disable/uninstall、显式 mount intent、快捷键、命令面板、原生菜单、数据目录迁移/备份/回退、MCP gate、退出保存。

终端协议细节（ANSI/UTF-8/alternate screen 等）只能作为上述现有能力的实现验收项，不能凭空扩展产品范围。

GPUI 交互：焦点与键盘导航、快捷键/动作、鼠标选区与滚轮、上下文菜单、窗口缩放、高 DPI、主题、字体 fallback、IME marked text/commit、剪贴板、可见性与滚动性能。

Herdr 集成：保留现有 Herdr 底层引用、进程/配置/事件/认证/业务功能；终端仅作为显示与输入壳，通过明确消息/命令接口接入，禁止把 Herdr 逻辑复制到 UI 层。

## 主要风险

- **IME**：Zed 源码显式维护 marked text 与 UTF-16 range，说明 GPUI 原生输入桥接不是“自动完成”；必须用中文、日文、韩文实机验证候选词、组合态、退格、粘贴。
- **Accessibility**：终端是自绘网格，系统辅助技术未必能读出每个 cell；GPUI/平台的可访问性覆盖需实测，不能以“能显示”推断 VoiceOver 可用。
- **API 漂移**：GPUI、Zed 内部 crate、gpui-kit 都可能破坏 API；commit pin + 最小适配层是必要条件。
- **性能**：避免每个 cell 一个 GPUI entity；批量绘制行/运行（run），按脏区和光标闪烁刷新；PTY 读取与解析放后台任务，UI 主线程只接收批量更新。
- **许可证/再分发**：GPL crate 的“直接依赖、复制代码、动态/静态链接、发布方式”结论不同，必须以最终 Cargo 图和法务审查为准。

## 验证计划（后续实现）

1. 锁定 GPUI、alacritty_terminal、gpui-kit 的 commit/tag，先建立最小窗口 + PTY + `echo` 自检；`cargo check` 通过才进入功能实现。
2. 用 shell 测试覆盖 ANSI、resize、alternate screen、鼠标、UTF-8/CJK/emoji、IME；再运行 goose-hub Herdr 真实流程验证底层引用未变。
3. 编译验收之外，补充 macOS VoiceOver/IME 与高 DPI 人工验收；将“编译成功”仅作为本调研任务的实现门槛，不冒充功能验收。

## 公开检索缺口与替代方案

- 本次公开检索未可靠获得 X.com 帖子及评论区全文；不伪造“人们推荐/评论区更强替代”的结论。另有代理负责 X 评论检索，结果应单独标注为社交证据，不替代官方源码证据。
- 未把其他终端库升级为新增业务方案；在当前约束下，独立 `alacritty_terminal` + 自有薄 PTY/GPUI 适配是最小、可控且不改变 Herdr 底层的选择。

## 来源索引（查阅日均为 2026-09-18）

- Zed terminal_view Cargo（观察日 2026-09-18；以该日抓取内容为准，不声称未来 web 发布状态）：许可证与内部依赖：https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal_view/Cargo.toml
- Zed terminal_view 实现：IME、焦点、滚动、菜单、workspace 耦合：https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal_view/src/terminal_view.rs
- Zed terminal Cargo：alacritty_terminal、GPUI、vte、libc 等：https://github.com/zed-industries/zed/blob/main/crates/terminal/Cargo.toml
- Zed terminal 实现：https://github.com/zed-industries/zed/blob/main/crates/terminal/src/terminal.rs
- Zed 仓库与许可入口：https://github.com/zed-industries/zed
- GPUI Kit 仓库（原 gpui-component 重定向）：https://github.com/longbridge/gpui-kit
- GPUI Component 站点（Apache-2.0 声明）：https://longbridge.github.io/


## 与 Herdr 契约的明确适配边界

依据 `planning/herdr-contract.md`：现有源码证据确认 Goose Run 的 PTY 由 libghostty/native addon 拥有，且 `term_ensure/spawn/inject/resize/dispose`、PID ownership、8 会话上限和 stop/dispose 顺序均有既定语义；Herdr 是否另有 daemon/session ownership 尚未从 goose-hub 源码确认。故 GPUI 层只实现：

- 终端解析/网格状态/渲染（可复用 `alacritty_terminal` 的解析能力，或保持现有 libghostty 输出）；
- 对既有 Herdr/terminal contract 的 attach、输入注入、resize、focus/visibility、dispose 适配；
- 不在未确认前 Rust `Command` 重新 spawn shell，不接管 PID，不复制 session store。

若后续 Herdr 源码证明 PTY 不由 Herdr 持有，再单独引入 PTY backend；否则这是双所有者错误。

## 版本/commit 锁定与许可证结论（截至 2026-09-18）

- Zed `terminal`/`terminal_view`：不建议作为依赖；其 manifest/仓库当前标注 GPL-3.0-or-later，且 workspace 内部依赖强。若复制或链接，按 GPL 再分发义务审查。
- GPUI：锁定 goose-herdr-gpui 已验证的 commit；不要写 `git = ...#main`。
- `alacritty_terminal`：锁定 Zed 当前 Cargo.lock 中实际解析的版本或精确 git revision，并提交 Cargo.lock；不要引用 Zed workspace path。
- gpui-component 已重定向至 gpui-kit；锁定具体 release/commit，并在 `cargo metadata` 后逐项审 license。公开页面与源码观察日均为 2026-09-18，不能推断 2026-09-18 之后的发布内容。

证据 URL：
- https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal/Cargo.toml
- https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal_view/Cargo.toml
- https://github.com/zed-industries/zed/blob/main/crates/terminal/src/terminal.rs
- https://github.com/longbridge/gpui-component
- https://github.com/longbridge/gpui-kit

## 2026-09-18 Herdr ownership correction

新增 `planning/herdr-contract.md` 实际证据后，必须区分两层所有权：

- Herdr daemon 是 PATH 中共享的 `herdr server`，由 HerdrKit 按需启动但不归 GUI 所有，daemon 不随 app 退出；其业务 PTY/pane 由 daemon 管理。
- GUI 的 `TerminalProcess` 是本地 shell 的 attach client PTY；Ghostty surface 负责渲染，Herdr attach sessions 与本地 shell sessions 可并存。禁止把“不要双 owner”误解为删除 GUI 必需的 client PTY。

因此“自有薄 PTY”精确定义是：**GPUI 可以保留与既有客户端等价的 attach/client PTY（仅在契约要求时），但不得再创建第二个 daemon/业务 PTY 或绕过 Herdr session ownership。** 默认实现应以 Herdr 的 newline-delimited JSON-RPC/Unix socket/SSH/Tailcat 协议兼容 client 为边界，复现 `session.snapshot`、`pane.get/read/send_input/close`、events subscribe、attach/reconnect 等语义；不应原封不动跨语言链接 Swift `HerdrKit`。真实 Herdr daemon、HerdrKit 版本与协议兼容测试通过后，才确定是否保留 client PTY 或仅使用流式 pane I/O。

用户决策门禁仍需明确列出两条路线：

1. **Zed GPL 路线**：直接抽取/复用 Zed terminal/terminal_view，接受 GPL-3.0-or-later 及 workspace 强耦合，法务与再分发审查通过后才可选。
2. **MIT/Apache 原创 adapter 路线**：只使用许可证允许的终端解析/GPUI 组件，原创 Herdr protocol-compatible adapter 与 GPUI 渲染层；这不是“仅套 Zed 皮肤”，而是独立实现 Zed 类终端行为。

`gpui-component` 与 `gpui-kit` 不应混称：公开仓库 `longbridge/gpui-component` 当前重定向到 `longbridge/gpui-kit`；报告中候选名称按该事实标注，最终以实际 Cargo 包 manifest 和锁定 commit 为准。

## 可落地的 Zed 终端复用路线（研究基线：2026-09-18）

### 已解析的上游基线

在 2026-09-18 观察到 Zed `HEAD=94c997e06faabe6538e608c6d04091914f02f86b`（这是当前源码观察基线，不是稳定 release 承诺）。官方 raw manifest 实际解析结果：

- `crates/gpui`：crate version `0.2.2`，license `Apache-2.0`；依赖 AccessKit、字体/窗口后端、渲染与异步运行时，虽可发布但 API 与 Zed 主仓同步变化。
- `crates/terminal`：version `0.1.0`、license `GPL-3.0-or-later`；直接依赖 `alacritty_terminal`、`gpui`、`vte`、`libc`，以及 Zed workspace 的 `collections/settings/theme/task/util/sysinfo/release_channel` 等。
- `crates/terminal_view`：version `0.1.0`、license `GPL-3.0-or-later`；依赖 `terminal`、`gpui`、`editor/project/workspace/db/language/ui/menu/settings/theme` 等，不能作为独立 widget。

证据（2026-09-18 抓取）：
- https://raw.githubusercontent.com/zed-industries/zed/94c997e06faabe6538e608c6d04091914f02f86b/crates/gpui/Cargo.toml
- https://raw.githubusercontent.com/zed-industries/zed/94c997e06faabe6538e608c6d04091914f02f86b/crates/terminal/Cargo.toml
- https://raw.githubusercontent.com/zed-industries/zed/94c997e06faabe6538e608c6d04091914f02f86b/crates/terminal_view/Cargo.toml
- https://raw.githubusercontent.com/zed-industries/zed/94c997e06faabe6538e608c6d04091914f02f86b/README.md

### 直接复用 Zed terminal 的最小适配设计（用户可选路线）

不 clone 整仓、不构建整 Zed 的前提下，可抽取 `terminal` crate 的终端状态/动作/解析相关源码，配套最小化其 workspace 依赖；`terminal_view` 则只抽取终端面板/滚动/选择/输入行为中需要的部分，不能直接 Cargo 引入。最小适配层应提供：

1. **Herdr client transport**：实现 Herdr newline-delimited JSON-RPC socket client、events subscribe、`session.snapshot`、`pane.get/read/send_input/close`、attach/reconnect；不调用 shell spawn。
2. **client PTY 注入点**：对本地 shell/attach client 保留现有 `TerminalProcess` 所需的 PTY/字节流；daemon 的 pane/业务 PTY 仍由 Herdr 所有。GPUI 只接收输出、写输入、resize 和事件。
3. **Zed terminal adapter**：将 Herdr byte stream 接到 `alacritty_terminal`/Zed terminal state；把 terminal state 的绘制、selection、scrollback、鼠标报告、IME、clipboard 和 action 映射到 GPUI。`terminal_view` 的 workspace/editor/db/project 依赖替换成 goose-herdr 的 session/sidebar/split tree model。
4. **GPUI host**：采用 GPUI `0.2.2` 作为研究基线（最终必须与仓库可编译 commit 一起锁定），提供窗口、焦点、绘制、AccessKit；不引入 Zed editor/workspace。

事实边界：Zed manifest 显示 `terminal` 仍大量依赖内部 workspace crates，`terminal_view` 更重；因此“直接复用”可落地形态是源码抽取+最小 adapter，不是把两个 crate 当公共 API 直接链接。GPL 许可不是自动否决，但该路线必须在用户选定后做逐文件许可/头部/分发审查；Zed README 明确其源码主要 GPL-3.0-or-later，标注部分 Apache-2.0 组件。

### 原创 MIT/Apache adapter 路线（同等功能目标）

不抽取 Zed GPL 代码，使用 Apache/MIT 许可的 GPUI、终端解析/状态库（最终以实际 Cargo lock 与 license audit 为准），原创实现上述同一层：Herdr protocol client、client PTY 注入、网格状态、选择/滚动/鼠标/IME/剪贴板、GPUI 绘制、split/session UI。优点是许可证与依赖边界更可控、可按 Herdr 语义设计；代价是需自行重建 Zed 的成熟交互与边界行为，首版风险和测试量更高。该路线不是“只做 Zed 皮肤”，而是完整等功能 adapter。

两条路线均必须覆盖 capability-audit/herdr-contract 中的真实能力；差别只在终端状态/交互实现来源与许可/维护负担。用户未决定最终 pin，故上述 `94c997e...` 与 GPUI `0.2.2` 仅作为可复现研究基线，不宣称稳定版本。
