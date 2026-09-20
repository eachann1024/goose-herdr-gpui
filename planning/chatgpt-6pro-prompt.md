# ChatGPT 6 Pro 外部复核 Prompt

> 2026-09-18：已核验 ChatGPT 思考强度为 `6 Pro`；已仅投递一次并取得完整回答；以下为脱敏投递正文，执行证据见 external-review.md。

你是本项目的最终架构复核者。请对下面这份**去隐私的能力与契约摘要**给出一份可直接执行的完整方案，而不是只列点评或建议。目标是一次性覆盖全部范围，不安排“二期”；所有未决项必须明确列出、给出阻塞条件、核验方法和默认处理，不得把未知写成已确认。

## 目标

将 Goose Hub 的现有能力迁移为 Rust + GPUI 的 macOS 客户端，并采用 Zed 风格终端体验，同时保持 Herdr 及其全部既有功能、协议语义、数据字段和错误行为不变。当前只做方案与原型，不执行代码改动。

## 当前范围决策

- 方案候选：S1（推荐）：保留 Herdr 业务与契约，借鉴 Hub 壳架构；S2：合并 Hub 全部业务与 Herdr。请比较边界、风险和迁移成本，并给出单一推荐。
- 交互原型：A 高保真工作台（推荐默认）、B 终端专注、C 多任务分栏；三者是同一能力模型的布局/偏好变体，不得删减功能。
- 两条终端路线均为候选：A 原创 `alacritty_terminal` + GPUI adapter（当前推荐，但需尊重既有三路 PTY ownership）；B 直接复用/抽取 Zed terminal/terminal_view 源码（须审查内部 workspace 依赖、GPL/逐文件许可证、固定 commit、上游同步补丁成本）。两路都完整比较，不把“Zed 做的终端”静默等同为仅参考视觉，也不预先宣布路线已经批准。
- PTY/Herdr 所有权必须避免双 spawn/双 owner；需区分 GUI client PTY、共享 Herdr server/daemon、attach/reconnect 与 session 生命周期。

## 必须保留的能力

范围未由用户确认。以下 S1、S2 必须分表，不得因为推荐 S1 而静默删减原请求中的 Hub 范围；推荐不等于用户批准。

S1 Goose Herdr：设备/workspace/space/agent session sidebar、分组拖拽排序与状态、snapshot/catalog/pane 操作、newline JSON-RPC/事件订阅/重连、agent 与 terminal attach/takeover/ended overlay/显式 reconnect、递归 split tree/比例/四向 focus 和 swap、Tailcat WireGuard/SSH fallback/local socket/stream-local forward、Keychain token、附件上传、命令生成、字体字号行距鼠标偏好和原生菜单。Swift HerdrKit 仅重写客户端协议适配，不改变 Herdr daemon/CLI/socket 契约；renderer 依赖版本不是 Herdr binary pin。

严格区分三路 owner：共享 daemon 的业务 PTY、attach client PTY、standalone shell PTY。GUI 退出不终止共享 daemon；attach client 退出不等于关闭 server session；standalone shell 自有生命周期。不能把 Hub Run 的 session/free、最多 8 和 libghostty native addon 所有权强行套到所有 Herdr session。

S2 条件范围（Hub 全业务与 S1 合并，不是二期）：

Hub：A/B/C 布局、商店浏览/下载校验/重试、已安装/启停/卸载、详情懒加载、挂载/卸载、启动恢复、快捷键录入/冲突/注册注销、app-only/launcher、命令面板、原生菜单、窗口生命周期、数据目录迁移/备份/回滚、主题、旧数据导入、MCP 适配。

组件与 Herdr：2FA（OTP/分组/导入/二维码/回收站/安全存储/MCP）、Marks（CRUD/分组/排序/搜索/模板 URL/导入导出/缓存/死链/同步/MCP；uTools-only 能力不得伪造）、Monitor（进程/窗口/端口/网络/PID 二次校验/分组/偏好/MCP）、Run（脚本 CRUD、分组/搜索/run/stop、cwd/env/shell/确认、兼容既有脚本与窗口数据）。

终端：shell/env/cwd、session/free、最多 8、resize、输入校验、SIGINT/EOF、VT/ANSI、scrollback、选择/复制/粘贴、搜索、alternate screen、鼠标、bracketed paste、OSC、bell、光标、重启、退出码、IME marked text/commit、剪贴板、窄窗降级、后台读取解析和脏区绘制。

## 强制门禁

1. 先审计 Herdr 的构建入口、版本/锁定依赖、协议与 framing、PID/process ownership、IPC、attach/reconnect、错误/超时、权限和许可证。
2. GPUI、Rust toolchain、终端库、PTY 后端必须 pin；生成 license/NOTICE。
3. 保留现有 schema、mounted intent、停止顺序、迁移白名单、空目录拒绝、staging/备份/原子切换/回滚。
4. 不把源码存在、编译成功、静态截图当作 runtime、IME、VoiceOver、MCP 或真实 Herdr 验收。
5. 安全边界：秘密不进日志；路径/权限/并发/超时/backpressure/断线均需定义；失败不得报成功。
6. 可访问性必须覆盖键盘焦点、label/role/name/state、reduced motion、CJK IME、复制撤销、高 DPI；自绘终端 VoiceOver 只能在实测后声明。

## 请输出

1. 选定的 S1/S2 与 A/B/C 默认方案，并解释为什么。
2. 一份按依赖排序的 1-All 实施计划，不能留“二期”。
3. Herdr、GUI、PTY、终端解析、storage/migration、MCP/native menu 的责任边界与时序图（文字即可）。
4. 全能力矩阵：每项能力的现状、目标承载、兼容性要求、验证方式和失败回滚。
5. 风险登记表：双 owner、协议不兼容、许可证、数据损坏、IME/a11y、性能、升级/降级，并给出可执行缓解措施。
6. 三个交互原型的评审标准与选择依据。
7. 迁移前门禁、编译门槛、runtime/视觉/IME/VoiceOver/MCP/真实 Herdr 分层验收清单。
8. 所有未决项：不要猜测，列出确认所需证据、阻塞级别和在证据缺失时的安全默认。
9. 对当前方案中最可能导致功能回归或数据损坏的前三个问题提出修订稿。

请将外部复核视为独立审查，不假设你能访问源码、私有仓库、私有截图、绝对路径、密钥、真实会话或账户信息；只基于本摘要作架构推理，并明确哪些结论必须回到源码/运行时验证。
