# Goose Hub → Rust + GPUI + Zed 风格终端完整实施计划

> 前序计划，当前整合结论以 [final-plan.md](final-plan.md) 为准。研究交付已完成；真实6 Pro原文见 [chatgpt-6pro-review.md](chatgpt-6pro-review.md)。用户未选型，未实施原生应用。

## 0. 决策边界

- **目标待定**：候选 S1（推荐）迁移 goose-herdr 的 SwiftUI/AppKit 业务与 HerdrKit 契约，借鉴 Hub 壳架构，不强行并入 Hub 无关组件；候选 S2 才是 Hub 全组件与 Herdr 合并。两者都不得删减各自范围功能，不能把 Hub 组件误写为 Herdr 需求。
- **推荐默认布局（未选定）**：A 高保真工作台；侧栏 + 中央工作区/终端 + 按需右窗格。B 终端专注、C 多任务分栏是同一模型的响应/偏好变体，不是删除 A 的功能。
- **终端技术推荐**：先保留 Herdr daemon/attach/client/standalone 三路所有权；Rust 只做协议兼容 adapter 与 GPUI surface，不凭空引入第二个 daemon PTY。Zed 终端代码复用仍待决。
- **Herdr**：保留 daemon/CLI/socket 协议、业务语义和数据字段；G0 由工具核实实际引用、版本、许可证与二进制，真实 Herdr binary 版本尚未 pin，不把 Ghostty renderer 版本当 Herdr binary pin。

## 1. 证据与门禁

1. 固定只读基线：保存源仓 `/Users/eachann/Work/goose-hub` 与目标仓状态、HEAD、目录清单；源树已有大量未提交改动，禁止覆盖或按其推断为目标实现。
2. 将 `capability-audit.md`、`herdr-contract.md`、`stack-research.md`、`native-design.md` 与原型/外部复核作为决策输入；每条结论带本地证据路径或官方 URL。
3. 迁移前门禁：Herdr 契约、数据 schema/迁移（仅 S2 另核 Run 可达性）、Zed/GPUI license、Cargo pin 均有可核查结论；未回答的问题保持“未决”，不得写成已定。
4. 不把静态截图、源码存在、编译成功当作 runtime/视觉/a11y 验收。

## 2. S1 主清单：goose-herdr 业务保真映射

S1 以 `/Users/eachann/Work/goose-herdr` 为业务基准；Hub 仅借鉴壳架构。HerdrKit 是 Swift 客户端契约，Rust 以协议兼容 adapter 重写客户端，不跨语言原封链接 SwiftKit；保留 Herdr daemon/CLI、socket、JSON-RPC、事件与 attach 语义。

| 现有 Goose Herdr 能力 | GPUI/Rust 承载 | 保留要求 |
|---|---|---|
| 设备、workspace/space、agent session sidebar | GPUI sidebar/state | 分组、拖拽排序、状态、快捷切换 |
| Herdr snapshot/catalog/pane 操作 | Rust Herdr JSON-RPC adapter | newline JSON-RPC、事件订阅、重连、协议版本检查 |
| agent/terminal attach | attach client + terminal surface | takeover、ended overlay、显式 reconnect |
| daemon 业务 PTY | 由 Herdr daemon 持有 | GUI 不终止共享 daemon |
| attach client PTY | attach 客户端按协议/渲染需要持有 | 不与 daemon PTY 混为一个 owner，不误删 |
| standalone shell | 独立 shell session | 与 daemon/attach 生命周期分离 |
| split tree、比例、四向 focus/swap | GPUI layout/action | 递归 split、键盘和菜单语义不变 |
| Ghostty terminal renderer | GPUI renderer adapter | 版本仅代表当前 renderer 依赖，不等于 Herdr binary 已 pin |
| Tailcat/SSH/local socket | Rust transport adapter | socket path、stream-local forward、WireGuard/SSH fallback |
| Keychain、附件、命令生成、错误重连 | Rust service + GPUI surfaces | 不因迁移终端删除 |

### 范围纠偏的源码依据

- Hub Run 的 `scriptId/free` 与 8 会话上限来自 `/Users/eachann/Work/goose-hub/components/run/src/terminal.ts:1,37`，不能作为 S1 Herdr 上限；S2 纳入 Run 时才保留对应契约。
- Hub envelope/watcher/串行队列来自 `/Users/eachann/Work/goose-hub/apps/host/src/main/storage/json-repository.ts:35-62`；真实 Herdr 的 retained spaces 使用 UserDefaults JSON（`/Users/eachann/Work/goose-herdr/Sources/GooseAgent/AppModel.swift:83-98`）。保留各自持久化行为，不推断二者同一 schema/机制。
- Hub 组件 MCP 注册来自 `/Users/eachann/Work/goose-hub/apps/host/src/main/plugins/component-mcp.ts:1-35`；这是 S2 组件宿主能力，不作为 S1 新增 MCP 宿主的依据。
- 共享 daemon 不归 GUI 所有且不得随 GUI 退出而终止：`/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/LocalServer.swift:119-125`。清理 attach client 与停止共享 daemon 是两回事。

### S2 条件附录（不作为 S1 二期）

若用户选择 S2，另行纳入 Hub 的插件商店、2FA、Marks、Monitor、MCP、Run、数据迁移等完整能力；这是范围切换，不是延后实现。其来源必须标记为 Goose Hub，不能写成 Herdr 现有能力。

## 3. 实施顺序（1-All，不留“二期”）

1. 冻结只读基线与证据索引；建立能力矩阵和“真实可达/预览/遗留/不可达”分类。
2. G0 只读核实实际 herdr daemon/CLI 引用、已有二进制、版本、许可证、进程/PID、IPC framing、attach/reconnect、错误/超时、权限边界；形成不可变 contract test。
3. 锁定 Rust toolchain、GPUI commit、协议 adapter 依赖与 Cargo.lock；Ghostty renderer 版本单独记录，不能当 Herdr binary pin；生成 license/NOTICE 清单。
4. 设计最小边界：GPUI window/layout/action；Herdr adapter；PTY/session；storage/migration；native menu（组件 adapter/MCP 仅 S2）；错误与日志（秘密不进日志）。
5. 实现窗口生命周期、single-instance、macOS close-hide、native menu、theme/token、窗口尺寸与 DPI。
6. 实现导航/工作区/overlay/命令面板/焦点恢复；按用户选定布局设置默认（推荐 A，尚未选定），接通 B/C 变体。
7. 实现 Herdr device/workspace/session catalog、pane 生命周期、attach/takeover、重连与 GUI/client 安全清理顺序；不停止共享 daemon。
8. 实现 Herdr 原生菜单、split/focus/swap、快捷动作、设置与终端偏好。
9. 实现 Herdr 配置、Keychain、附件与连接状态迁移；先 dry-run，再原子切换。
10. 实现 local/SSH/Tailcat transport、Keychain token、附件上传及错误状态，不复制 Herdr 业务。
11. 实现终端：PTY ownership 以 Herdr contract 为准；保留 Herdr 的 shell/env/cwd、session/pane 标识、resize、输入校验、SIGINT/EOF、VT/ANSI、scrollback、选择复制粘贴、搜索、alternate screen、鼠标、bracketed paste、OSC、bell、光标、重启、退出码。
12. 实现 GPUI 终端交互：批量绘制/脏区、后台读取解析、焦点、滚轮/选择、IME marked text/commit、剪贴板、上下文菜单、可见性/遮挡、窄窗降级。
13. 保持 daemon PTY、attach client PTY、standalone shell 三路 owner 分离；不得引入第二个 daemon 或由 GUI 终止共享 daemon。
14. 接入 Herdr 事件订阅、断线重连、backpressure 与清理；测试 attach client 退出不误杀 daemon。
15. 安全迁移：保留实际 Herdr 配置与 Keychain 语义，校验路径/权限、保留备份、失败不报成功并能回滚；仅 S2 按 Hub 原契约保留原子 envelope、writer queue、watcher，不为 S1 凭空增设存储机制。
16. 可访问性与交互验收：键盘全路径、焦点可见、label/role/name/state、reduced motion、中文/日文/韩文 IME、复制撤销、长文本/高 DPI；VoiceOver 自绘终端语义仅在实测后声明。
17. 发布：构建/签名/打包、依赖许可证与 NOTICE、升级/降级/恢复、崩溃/日志脱敏、数据备份；建立回滚开关与旧数据恢复指引。
18. 按用户约定，编译成功即通过实现验收；runtime、真实 Herdr、视觉、IME、VoiceOver、迁移（MCP 仅 S2）另列已测/未测，不追加为用户验收条件，也不冒称编译证明这些能力。


## 3a. Zed 终端路线（两条可选，均需最终 pin）

研究基线为 Zed 源码 commit `94c997e06faabe6538e608c6d04091914f02f86b`，不是稳定 release，也不代表 Herdr 版本或目标工程已编译通过的依赖组合；当前目标目录尚无 Cargo.toml，最终依赖 pin 待实现时确定。

- **直接复用/抽取路线**：以该 commit 的 `gpui`、`terminal`、`terminal_view` manifest 建立 workspace 依赖图；`terminal` 依赖 `alacritty_terminal`、`gpui`、`vte`、`libc` 及 Zed 内部 `settings/theme/task/...`，`terminal_view` 还牵涉 editor/project/workspace。不能把它当独立公共 crate；需抽取源码、替换 workspace 依赖、固定 API pin、保留逐文件 license/header，并完成 GPL 与分发义务审查；上游同步按 commit diff 和适配补丁维护。
- **原创 adapter 路线**：使用许可证可接受的 GPUI/终端组件，原创 Herdr protocol adapter、client PTY 注入、网格/选择/滚动/鼠标/IME/剪贴板和 GPUI 绘制。依赖边界较小但测试与行为重建成本更高。

两条路线都不得改变 Herdr daemon/CLI/socket 协议；最终选择、Cargo.lock、许可证和上游同步策略是未决门禁。

## 4. Grilling：实施前的3个设计选择（不阻碍研究交付）

这些是实施前的设计选择，不是报告交付前提；环境事实由工具查证，完整当前步骤见 final-plan。

1. **业务范围**：是否以 `/Users/eachann/Work/goose-herdr` 为业务基准、Hub 仅借壳（S1）？**推荐 S1**；S2 全 Hub 合并是未决范围切换。
2. **Zed 复用与许可**：是否接受直接复用 Zed GPL 代码？**推荐原创 GPUI adapter，不嵌 `terminal_view`、不 fork；逐项审查许可。**
3. **默认交互方案**：A/B/C 选哪个默认？**推荐 A 高保真工作台，B/C 为同一模型变体。
6 Pro入口已确认且完整复核已完成，不再列为用户未决问题。

## 5. 三个交互原型

- [`prototypes/index.html`](prototypes/index.html)：S1 Herdr 业务的 A 高保真工作台、B 终端专注、C 多任务分栏；原型只验证信息架构与流程，不证明 GPUI 行为。
- 推荐选择 A；选择依据：设备/workspace/session/attach/终端能力最完整可发现，B/C 不丢 S1 功能。
- 原型内联 SVG 流程覆盖 daemon → attach client → GPUI renderer，并演示连接、重连、split、附件、通知和命令面板；S2 Hub 安装/启停/迁移/MCP 流程仅在条件范围确认后追加。

## 6. 外部复核输入

已按脱敏摘要完成一次真实 `6 Pro` 复核，完整最终原文见 [chatgpt-6pro-review.md](chatgpt-6pro-review.md)，执行/关闭证据见 [external-review.md](external-review.md)。整合采纳与事实纠偏见 [final-plan.md](final-plan.md)；不改外部原文、不把推荐当用户批准。

## 7. 证据索引

- `/Users/eachann/Work/goose-herdr-gpui/planning/capability-audit.md`
- `/Users/eachann/Work/goose-herdr-gpui/planning/herdr-contract.md`
- `/Users/eachann/Work/goose-herdr-gpui/planning/stack-research.md`
- `/Users/eachann/Work/goose-herdr-gpui/planning/native-design.md`
- Goose Hub AGENTS：`/Users/eachann/Work/goose-hub/AGENTS.md`
- Zed terminal_view Cargo：https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal_view/Cargo.toml
- Zed terminal 源码：https://github.com/zed-industries/zed/blob/main/crates/terminal/src/terminal.rs
- GPUI Kit：https://github.com/longbridge/gpui-kit
