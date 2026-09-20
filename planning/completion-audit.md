# 完成审计：Hub 扫描 → Herdr 保真 GPUI/终端方案

## 当前最终结论（2026-09-18，覆盖下方历史状态）

**本轮研究、完整方案与选择原型交付完成。** 当前单入口为 [final-plan.md](final-plan.md)。下方旧“6 Pro未执行”、旧原型缺口和未完成结论保留作历史，不再描述当前状态。

- Hub扫描、真实Herdr源码契约、GPUI风格、X主帖/回复实读及官方源码研究已具备对应报告；最终版补入真实Files工作区，S1/S2能力矩阵、两终端路线、完整1-All、迁移/新数据保护回滚、风险/依赖基线齐全。
- 真实 `6 Pro` 已完成一次脱敏提交并生成最终9节/1-All回答：[完整原文](chatgpt-6pro-review.md)；[执行记录](external-review.md)记录16m48s、完成UI与浏览器空间关闭。原文不改；推荐S1+A+TA不是用户批准。
- 已纠偏：Hub Run的session/free/8与Hub MCP/存储机制不套S1；A仍推荐未选；三路owner保留；不强造broker/actor框架；Herdr未pin、Zed研究commit不是目标已编译版本；不将外部全套门禁强加为用户验收标准。
- 原型A/B/C、流程图、比例range、Tab活动pane、模拟发送/附件/远程连接、命令过滤与a11y标记已具备；`node planning/prototypes/check.mjs`本次输出 `prototype behavior/structure check: ok`。检查执行部分纯函数并做结构断言，不是全DOM/浏览器视觉或真实Herdr验证。
- 本地Markdown文件链接校验通过（45处，仅本地文件存在性，不校验远端URL或锚点）；没有Rust工程编译、真实Herdr/runtime、IME、VoiceOver或浏览器视觉验收声明。
- S1/S2、A/B/C、TA/TB未选属于实施前设计决定，不阻碍本轮报告完成；按用户约定，后续编译成功即实现验收通过，额外运行风险只按实测状态报告。

### 历史审计记录（以下均为当时观察）


日期：2026-09-18。仅审计 planning、`/Users/eachann/Work/goose-hub` 与 `/Users/eachann/Work/goose-herdr` 的权威源码引用；未初始化目标工程、未启动 Herdr/daemon、未做浏览器视觉验收。旧 `review-findings.md` 的“通过”不作为本审计证据。

## 要求 → 证据 → 状态

| 要求 | 当前证据 | 状态 / 缺口 |
|---|---|---|
| 扫描 Hub 宿主架构、A/B/C 布局、真实入口 | `capability-audit.md:5-13`，引用 Hub `HubShell.tsx`/`useHubModel.ts` | **完成（文档证据）**；未做运行时验证 |
| 列出 Hub 全部具名可达能力 | `capability-audit.md:15-34`：商店、安装/启停、挂载、恢复、快捷键、命令面板、菜单、窗口、迁移、主题、导入、MCP、SDK IPC、安全 | **完成（扫描证据）** |
| 区分 2FA/Marks/Monitor/Run 业务边界与暂缓状态 | `capability-audit.md:35-47,57-64` | **完成**；Run 仍需用户决定是否纳入 S2 |
| 扫描 Herdr 设备/workspace/session/split/attach/SSH/Tailcat/附件/Keychain | `herdr-contract.md:44-52,87-92`；`full-plan.md:23-34` | **完成（源码引用）**；未做真实连接验收 |
| S1/S2 不混写 | `full-plan.md:19-38` 已有 S1 主清单、S2 条件附录 | **完成（计划边界）**；用户尚未选择 scope，不能擅自开始实现 |
| 保持 daemon/attach client/standalone 三路 PTY owner | `full-plan.md:26-30,52-55`；`herdr-contract.md:77-85`；`stack-research.md:107-114` | **完成（契约边界）**；尚无 contract test/真实进程验证 |
| Rust/GPUI 不直接链接 Swift HerdrKit、采用协议 adapter | `full-plan.md:21,26,33`；`herdr-contract.md:46-50` | **完成（设计决策）**；JSON-RPC 兼容性、协议版本和二进制 pin 仍待 G0 |
| 保留 Herdr CLI/socket/JSON-RPC/events/attach 语义 | `full-plan.md:21,26-34,43-55` | **部分完成**：能力映射完整；仍缺正式消息/错误/超时/backpressure contract test |
| Zed 风格终端不是只换皮 | `full-plan.md:9,52-55`；`native-design.md:59-66,75-94`；`stack-research.md:7-20,43-65` | **部分完成**：终端行为、IME、焦点、选择、滚动、ANSI 等有清单；若选择直接复用 Zed crate，完整 GPL/workspace/API/同步/再分发路线尚未写成可执行方案；当前推荐原创 adapter，需保持该推荐明确 |
| Zed GPL/许可证边界 | `stack-research.md:15-20,30,41,53-59,93-98`；`full-plan.md:44,66` | **部分完成**：已说明 GPL 风险与不直接嵌入推荐；缺最终 `cargo metadata`、lockfile、NOTICE 和法务/再分发结论 |
| Grilling 全部步骤 | `full-plan.md:61-68` 四项：scope、Zed许可、A/B/C、6Pro入口 | **部分完成**：问题/推荐答案齐全；用户尚未回答，不能宣称决策完成 |
| 三版 HTML A/B/C | `prototypes/index.html` 有 `data-layout=a/b/c`、布局 CSS 与按钮 | **部分完成**：A/B/C 确实改变侧栏/右栏/列宽和标题；但 C 没有独立的多任务分栏交互，仍依赖同一初始两 pane，A/B/C 主要是壳布局差异 |
| 三版保持同一业务可达 | HTML 共用设备、workspace、session、terminal、split、attach、通知、附件、⌘K | **完成（静态逻辑）**；按钮行为并非全都真实完成 |
| 原型核心交互无无效按钮 | `index.html`：`send` 按钮无 handler；`.inputbar` 动态 clone 后也无发送 handler；初始 `.close` 有 handler；`attach/notify/file` 仅打开说明 modal；`settings` 仅打开设置标题 modal | **未完成**：发送、通知/附件/远程连接确认动作只是展示；应标为“演示占位”或补最小模拟反馈，避免把无动作按钮当核心流程 |
| 原型 check 覆盖边界清楚 | `prototypes/check.mjs` 只检查 A/B/C 标记、`console.assert`、`aria-label`、daemon 文案 | **部分完成**：能证明结构存在；不执行 HTML JS、不验证按钮 handler、三布局差异、无外发请求、固定演示数据、键盘/Escape/modal 行为。当前 `node` 自检通过只代表这些正则断言通过 |
| 原型无 iframe/外发/动态数据 | 静态扫描 `index.html` 未见 iframe、fetch、XHR、WebSocket、外部 URL；演示文本固定 | **完成（静态扫描）**；不是浏览器 CSP/网络运行时证明 |
| 流程图覆盖用户要求的 Hub 全流程 | `full-plan.md:72-74` 仍写“首次导入 → 工作区 → 安装/启停 → 组件 → 命令面板 → 迁移 → 终端/Run → MCP → 停止/恢复/回滚” | **不足/范围混淆**：这是 S2/Hub 流程，但当前原型是 S1 Herdr；应提供 S1 Herdr 流程图，或明确该流程仅为 S2 附录，不能声称当前 A/B/C 已覆盖安装/启停/MCP |
| chatgpt-6pro prompt 可用且无泄露 | `chatgpt-6pro-prompt.md:1-47`；无绝对路径、密钥样式、账户/会话信息；`external-review.md` 明确未投递 | **完成（脱敏稿）**；GPT-6 Pro 未执行，不能算外部复核 |
| X 评论/推荐核验 | `external-review.md:41-43` | **未完成但已诚实标注**：无可读具体 X 帖文，不得假装完成 |

## 原型静态逻辑审计

- `node planning/prototypes/check.mjs`：通过，输出 `prototype structure/link/script check: ok`。
- A/B/C：`layout()` 仅切换 `.body` 的网格列；A 为侧栏+中央+右栏，B 隐藏两侧栏，C 为窄侧栏+宽右栏。三者共享同一 DOM 和业务控件，因此满足“同一业务模型”，但 C 没有把 pane 分栏作为独立布局状态展示，不能完整证明“多任务分栏”方案。
- 无外发：未见 `iframe`、`fetch`、XHR、WebSocket、`window.open` 或外部 URL；数据为 HTML 固定文本/固定状态。
- 交互缺口：终端“发送”按钮无事件；动态 split pane 的发送按钮同样无事件；附件/通知/远程连接只打开静态 modal；命令菜单搜索框无过滤逻辑；初始 pane 关闭确认依赖 `confirm`，不适合作为可访问性/真实流程证据。上述不影响信息架构原型，但必须在交付说明标为占位，不能称核心流程已可用。

## 精确残缺清单

1. **P1（方案一致性）**：把 `full-plan.md:72-74` 的 Hub/S2 流程明确拆出 S1 Herdr 流程；否则“完整 1-All”会把未选择的 S2 安装、MCP、迁移能力混入 S1 原型。
2. **P1（技术路线）**：在 `stack-research.md` 或 `full-plan.md` 明确“当前推荐原创 GPUI + Herdr protocol adapter”；若用户选择“完整原样复用 Zed terminal crate”，必须补齐 workspace 依赖图、GPL 衍生/再分发、API pin、上游同步和构建路线，不能只写 GPL 风险。
3. **P2（原型）**：给 C 增加可见的多 pane/分栏状态或在说明中承认 C 当前仅是布局草图；为无 handler 的发送/附件/通知/attach/搜索动作加演示反馈或明确标注占位。
4. **P2（验证）**：增强 `check.mjs` 的静态检查，至少断言三布局 CSS 差异、无 `iframe`/网络 API、固定演示标记、关键按钮存在；仍不得把它写成视觉或真实 Herdr 验收。
5. **P2（能力矩阵）**：`full-plan.md` 的 S1 表已覆盖 Herdr 能力，但 Hub 全具名能力仅在 `capability-audit.md`，且 S2 附录以一句话概括；若用户最终选择 S2，必须把 Hub 每项能力逐项映射，不得用“完整能力”代替矩阵。

## 当前判定

**未完成最终审计闭环。** 规划研究与 S1/S2 边界、三路 owner、协议 adapter 方向已有足够证据；但存在 P1 流程 scope 混淆、Zed 完整复用路线不足，以及 P2 原型占位/检查覆盖不足。用户尚未确认 scope/布局属正常未决，不可初始化工程或假称迁移完成。

## 定向复审更新（2026-09-18）

### 已核实修复

- `full-plan.md` 已补 S1 Herdr 流程与完整 Zed 直接复用路线；`stack-research.md` 已记录 Zed HEAD `94c997e...`、GPUI `0.2.2` 研究基线、`terminal`/`terminal_view` GPL 与内部 workspace 依赖，以及“源码抽取+最小 adapter”而非直接 Cargo 链接。旧 P1 方案缺口可关闭，但最终用户仍须选择 GPL 路线或原创 adapter 路线。
- 原型已增加第三个可见 pane；发送按钮和 Enter 会追加明确的 `[模拟输入]`/`command accepted (no backend)`；通知、附件、远程 attach 均标为模拟，附件只显示固定 `example-log.txt`，不选择、不上传；命令搜索会隐藏不匹配项。
- `node planning/prototypes/check.mjs` 仍通过。`external-review.md` 已补 3 条 X 帖/回复实读摘要；不再将其描述为“完全未读”，但仍不是工程事实或官方验证。

### 原型行为复核（静态 JS 控制流）

- A/B/C 切换只改 `#body` 的 class 与标题；设备、session、pane、连接状态和 modal DOM 共享。切换不会持久化到 localStorage，也不会重建 pane；已新增/关闭 pane、已输入的模拟输出和连接状态会跨布局保留。这符合同一业务模型，但不是布局选择持久化功能。
- A 是三列；B 将侧栏/右栏宽度置零；C 是窄侧栏+宽右栏并保留三 pane。C 的多 pane 现在可见，但没有拖拽分隔线、pane 选中/键盘切换或比例调整，因此仍只是流程原型，不是 split tree 验收。
- 初始与动态 pane 的发送/Enter handler 已挂载；输入为空时静默返回，非空追加模拟文本，明确无 backend。不存在真实 Herdr/PTY/网络调用。
- 远程连接按钮只改变 selection/state 文本为 `remote attached (simulated)`；通知/附件打开 modal 并写入固定文本。应继续把这些标成演示，不得描述为文件选择、通知读取或真实 attach。
- `⌘K`、Escape、modal focus 和命令搜索存在；关闭 pane 依赖浏览器 `confirm()`，错误/取消路径仅为浏览器原生演示。
- 无 `prefers-reduced-motion` CSS 或等价动效开关；当前 CSS 仅有短 transition。这不影响原型交付，但不能称 reduced-motion 已覆盖。× 关闭按钮没有 `aria-label`，而输入仅有 terminal `aria-label`；这是原型级 P2 可访问性缺口。

### 当前剩余门禁

1. 用户仍未确认 S1/S2、A/B/C 默认；未确认不代表失败，也不能初始化工程。
2. GPL 直接复用路线虽已具备研究基线和适配设计，仍需用户选择并完成逐文件许可、Cargo lock、NOTICE、构建与分发审查；原创 adapter 仍是推荐备选。
3. GPT-6 Pro 仍未执行；X 帖/回复仅作为已实读的体验线索，不能替代源码、官方或 runtime 证据。
4. 仍未做浏览器视觉验收、真实 Herdr attach/daemon、编译、IME、VoiceOver 或运行时协议测试；这些是未来实施门禁，不是本轮方案/原型交付失败。

### 可安全自行完成 vs 必须用户输入

- **可自行完成**：把 `check.mjs` 增强为检查三布局 CSS、关键 handler、无 iframe/网络 API、固定模拟标记；为关闭按钮补 `aria-label`、补 `prefers-reduced-motion`；在 full-plan 中把 S1 与 S2 流程图链接分开；把 `verification.md` 作为“node --check + 结构校验、非浏览器验收”说明。
- **必须用户输入/授权**：选择 S1 或 S2；选择 A/B/C 默认；是否接受 GPL Zed 源码抽取/再分发路线；是否允许在未找到 GPT-6 Pro 入口时使用其他实际模型。当前不能把这些未决项标为完成。

**定向复审结论：方案/原型范围内基本通过，保留 P2 原型 a11y/验证覆盖项；无须因尚无 Rust 工程或真实 Herdr runtime 而判定本轮交付失败。**

## 最终 P2 复审（2026-09-18）

已执行 `node planning/prototypes/check.mjs`，输出 `prototype behavior/structure check: ok`。这次检查确实使用 Node `vm` 执行了 `layoutClass`、`filterCommands`、`appendOutput`，并断言 reduced-motion、pane label、range、无网络 API；不是仅正则通过。但仍发现两处断言覆盖之外的真实逻辑缺口：

- `#ratio` 的 handler 只执行 `document.documentElement.style.setProperty('--split', ...)`，CSS 中没有任何 `var(--split)` 或使用该变量的 `grid-template-columns/flex-basis`；因此 range 改值不会改变 C 的 pane 比例。当前 check 没覆盖这一点。精确修复：让 C 的分栏 CSS 使用 `var(--split)`（例如 `body.c .pane:first-of-type { flex-basis: var(--split, 50%); }`），或删掉 range 并明确它只是静态控件。
- pane 有 `tabindex="0"`/`role="region"`，可被 Tab 聚焦，但没有 `focus` 事件或 active-pane 状态更新；因此“Tabfocus 会激活 pane”尚未实现。精确修复：绑定 pane focus 事件更新活动 pane/标题/状态，或将文档表述改为“可 Tab 聚焦”，不要称为 pane 激活。

已解决且不再列为缺口：`prefers-reduced-motion`、关闭 pane 的 `aria-label`、图标按钮名称、第三 pane、发送/Enter 模拟、命令过滤、附件/通知/attach 的明确模拟标记。以上仍仅原型逻辑，不代表真实 Herdr、视觉或 VoiceOver 验收。

当前无需继续自行修改的用户门禁：S1/S2、A/B/C 默认、GPL Zed 复用路线，以及 GPT-6 Pro 是否有可用入口。GPT-6 Pro 仍未执行；真实 Herdr/runtime、浏览器视觉、IME、VoiceOver 属未来实施验收。

## 最终 P2 关闭复核（2026-09-18）

已执行 `node planning/prototypes/check.mjs`，输出 `prototype behavior/structure check: ok`。新增断言通过且覆盖真实逻辑：Node `vm` 执行布局类名、命令过滤、模拟输出和 `setActivePane`；结构断言检查 `--split` CSS 消费、range 写值、`focusin`、reduced-motion、pane label 和无网络 API。

- C 布局现在由 `.body.c #panes { grid-template-columns: var(--split,50%) calc((100% - var(--split,50%))/2) calc((100% - var(--split,50%))/2) }` 消费 range 值；range `25..75` 会改变第一列宽度，其余两列平分剩余空间，比例逻辑有效。
- `.pane` 绑定 `focusin`，调用 `setActivePane`，只切换 active class，不抢焦点；Tab 可聚焦 pane 后会更新活动 pane。该行为由 vm 断言验证。

此前两项 P2 已解决：range 不生效、Tab focus 不激活 pane。当前本轮不再有需自行修改的原型逻辑问题。

剩余仅为用户 scope/布局选择、Zed GPL 或原创 adapter 路线选择、GPT-6 Pro 是否存在可证明入口；真实 Herdr、视觉、IME、VoiceOver 仍是未来实施验收，未在本轮冒称完成。
