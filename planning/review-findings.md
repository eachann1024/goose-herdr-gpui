# Planning 复核发现

## 当前最终结论（2026-09-18，覆盖下方历史状态）

**本轮研究、完整方案与选择原型交付完成。** 当前单入口为 [final-plan.md](final-plan.md)。下方旧“6 Pro未执行”、旧原型缺口和未完成结论保留作历史，不再描述当前状态。

- Hub扫描、真实Herdr源码契约、GPUI风格、X主帖/回复实读及官方源码研究已具备对应报告；最终版补入真实Files工作区，S1/S2能力矩阵、两终端路线、完整1-All、迁移/新数据保护回滚、风险/依赖基线齐全。
- 真实 `6 Pro` 已完成一次脱敏提交并生成最终9节/1-All回答：[完整原文](chatgpt-6pro-review.md)；[执行记录](external-review.md)记录16m48s、完成UI与浏览器空间关闭。原文不改；推荐S1+A+TA不是用户批准。
- 已纠偏：Hub Run的session/free/8与Hub MCP/存储机制不套S1；A仍推荐未选；三路owner保留；不强造broker/actor框架；Herdr未pin、Zed研究commit不是目标已编译版本；不将外部全套门禁强加为用户验收标准。
- 原型A/B/C、流程图、比例range、Tab活动pane、模拟发送/附件/远程连接、命令过滤与a11y标记已具备；`node planning/prototypes/check.mjs`本次输出 `prototype behavior/structure check: ok`。检查执行部分纯函数并做结构断言，不是全DOM/浏览器视觉或真实Herdr验证。
- 本地Markdown文件链接校验通过（45处，仅本地文件存在性，不校验远端URL或锚点）；没有Rust工程编译、真实Herdr/runtime、IME、VoiceOver或浏览器视觉验收声明。
- S1/S2、A/B/C、TA/TB未选属于实施前设计决定，不阻碍本轮报告完成；按用户约定，后续编译成功即实现验收通过，额外运行风险只按实测状态报告。

### 历史审计记录（以下均为当时观察）


复核日期：2026-09-18。范围：只读审阅现有 planning 文件及 `/Users/eachann/Work/goose-hub`、`/Users/eachann/Work/goose-herdr` 的已引用源码证据；未启动 Herdr/服务，未修改业务代码、源仓或分支。`GPT-6 Pro 复核`仍未执行，不能视为外部通过。

## P1

### P1-1：S1 推荐与“1-All”执行清单仍混入 S2 能力，无法作为当前实施边界

- **证据**：`planning/full-plan.md:7` 把 S1 定义为 goose-herdr 业务、S2 才是 Hub 全组件；但 `full-plan.md:19-41` 的 All 能力表直接把 Hub 商店、插件安装/启停、2FA、Marks、Monitor、MCP 全列为目标；`full-plan.md:51-58` 又要求实现插件 catalog/store、2FA、Marks、Monitor、Hub MCP。
- **影响**：即使正文说“候选待定”，执行者按“1-All，不留二期”会默认做 S2；这与 `README.md:3,5-10` 的 S1 推荐和“不能二者混写”冲突，导致工作量、信任边界和验收范围不可判定。
- **修复**：在用户确认前，把 All 清单拆成明确的 `S1-All` 与 `S2-All` 两棵清单；S1 只保留 Herdr/GooseAgent 当前业务、HerdrKit、终端 split、设备/workspace/agent、附件/SSH/Tailcat、菜单/快捷键等真实能力，Hub 插件/商店/2FA/Marks/Monitor/MCP 标为 S2 候选，不列入 S1 实施步骤。每个原型也必须标明 scope，不能只写同一能力模型。

### P1-2：PTY 所有权结论与推荐实现相互冲突，存在双 spawn/双 owner 风险

- **证据**：`planning/full-plan.md:9` 推荐“独立 `alacritty_terminal` + 极薄自有 PTY/GPUI 适配”；`planning/stack-research.md:9-13,83-91` 虽提醒不能默认自有 PTY，但仍把自有 PTY 作为主推路径；真实 Herdr 证据 `planning/herdr-contract.md:77-85` 明确：HerdrKit 可按需启动共享 `herdr server`，而 `GooseAgent/TerminalView.swift:270-305` 的本地 shell 由 `TerminalProcess` fork/exec PTY，远端则经 `pane.*`/attach 协议，Ghostty 负责渲染。
- **影响**：当前计划没有按“远端 Herdr pane / 本地 shell / attach takeover”分别定义 owner、数据流和生命周期。直接接入自有 PTY 会与本地 shell 或 daemon/session ownership 重复；直接用 `alacritty_terminal` 也不能替代现有 Ghostty/Herdr pane 协议。
- **修复**：把终端方案改成三路明确契约：① Herdr attach：socket JSON-RPC + `events.subscribe` + `pane.get/read/send_input`，GPUI 仅渲染/输入；② 本地 shell：复用现有 `TerminalProcess`/Ghostty owner，或先做经批准的替换实验，不能默认重 spawn；③ session attach/reconnect：保留 `--takeover`、ended/reconnect 和 opacity 保活语义。只有源码/用户确认 owner 后，才决定是否引入 `alacritty_terminal`，并从推荐结论中删除“自有 PTY”歧义。

### P1-3：S1 的“SwiftUI/AppKit 业务迁移”没有说明 GPUI 与 HerdrKit 的跨语言边界

- **证据**：`planning/full-plan.md:7,10` 同时要求迁移 SwiftUI/AppKit 业务、HerdrKit 契约不变，并以 Rust/GPUI 替换宿主；`planning/herdr-contract.md:44-52,80-92` 说明 HerdrKit 是 Swift Package，真实业务通过 Swift `HerdrService`、Keychain、SSH/Tailcat 和 Ghostty/NSView 完成。
- **影响**：当前计划把“底层引用不改”和“Rust/GPUI 实现”并列，却没有规定 Swift Package 是保留为 helper/XPC/CLI，还是做 C ABI/JSON-RPC bridge；实现者会擅自重写 HerdrKit 或伪造协议，违反“功能不变”。
- **修复**：在 G0 增加唯一跨语言决策：保留现有 GooseAgent/HerdrKit 作为进程内 Swift helper、独立 helper/XPC，或先不迁移 Swift UI 仅做 GPUI 壳。每个选项补充构建入口、ABI/IPC、崩溃/退出、Keychain、SSH/Tailcat 和 Ghostty NSView 归属；未选定前不得进入终端实现步骤。

## P2

### P2-1：Herdr “真实 pin/版本”门禁表述不够精确，已知与未知混在一起

- **证据**：`planning/herdr-contract.md:71-85` 已确认 `project.yml` 的 Swift 版本、Ghostty `1.6.20260909` 与 resolved revision；但 `herdr-contract.md:56-63,99` 仍将 upstream/fork、HerdrKit 版本/ABI、daemon/PID/错误协议全部笼统列为未确认；`full-plan.md:46-47,68` 也只写“补齐”。
- **影响**：审阅者无法区分“已锁定的本地依赖”与“必须向用户确认的 CLI/daemon 发布物”，容易重复调查或误把 Ghostty pin 当作 Herdr pin。
- **修复**：增加表格列 `已确认证据 / 未确认事实 / owner / 阻塞步骤`：单独记录 HerdrKit 本地 Package 无 semver、Ghostty exact version/revision、`herdr` CLI PATH 来源及其 commit/version 未确认；G0 只阻塞后两者和跨语言边界，不再笼统写“Herdr 版本未确认”。

### P2-2：原型文件状态与计划引用不一致，验收链接会断

- **证据**：`planning/full-plan.md:73-77` 引用 `prototypes/index.html`，但当前目标仓 `find` 仅发现 `planning/*.md`，没有 `prototypes/index.html`；`README.md:25` 也称原型与流程 canvas 尚待文件到位。
- **影响**：计划把不存在的三版 HTML 当输入/交付物，无法按链接复核。
- **修复**：在原型文件落地前把链接改为“待生成（不作为当前证据）”，或由原型代理先生成并提交 `prototypes/index.html` 后再把它列入实施入口；同时在 review 中标注三版 HTML 尚未验收，不能据此作视觉结论。

### P2-3：Hub 能力表对 S1/S2 的来源标注不足，易把“Hub 真实能力”误传为“Herdr 能力”

- **证据**：`planning/capability-audit.md:35-47` 已区分四个 Hub 组件与 Run 缺口，但 `full-plan.md:21-41` 的表头写“现有能力”而不注明来源；`full-plan.md:54-57` 将 Hub 组件与 Herdr 终端步骤连续排列。
- **修复**：能力表新增 `来源（Hub / Herdr / Goose Run）` 与 `S1/S2` 列；Herdr 的设备、workspace/space、agent catalog、Tailcat/SSH、附件和 split tree 单独列出，避免用 Hub 的 catalog/install 语义覆盖 Herdr。

## 未构成阻塞的保守结论

- `planning/external-review.md:21-39` 对 GPT-6 Pro 的记录是“未找到可证明入口、未执行”，表述正确；不得改写为已复核。
- `planning/native-design.md:64-66,87-98` 对 GPUI 自绘 a11y、IME、VoiceOver 保持“待验证”，没有发现把截图当作验收的越界。
- 未发现需要启动 Herdr、服务或修改源仓才能完成本轮复核的事项。

## 复审更新（2026-09-18）

### P1-2 复核结论：原问题需收窄，不能把 GUI client PTY 等同于 daemon owner

对真实 Herdr 源码复核后，原 P1-2 的“双 owner”表述过宽，已修正为以下唯一边界：

1. **Herdr daemon 是远端/本机共享会话的唯一业务 PTY owner**：`HerdrKit/LocalServer.swift:52-79,119-163` 显示 GUI 只按需启动不归 app 所有的 `herdr server`；注释明确 daemon 持有所有 agent PTY，并且 app 退出不终止它。
2. **GUI attach client 可以有自己的本地 PTY**：`HerdrService.swift:801-847` 生成 `herdr agent attach ... --takeover` / `terminal attach ... --takeover` 命令，`TerminalView.swift:270-313` 的 `TerminalProcessHost` 负责承载这个本地 TUI/client 进程。该 client PTY 是 attach 流的本地传输/显示端，不是 Herdr daemon 的业务 session PTY；存在本身不构成双业务 owner。
3. **本地 standalone shell 是另一条明确路径**：`HerdrService.swift:719-767` 生成本地 `/bin/sh` 或 SSH `-tt` 命令；它只属于本地 shell pane，不得被误写成 Herdr daemon session。
4. **GPUI 迁移边界**：GPUI 可替换当前 SwiftUI/AppKit/Host surface，但必须保留“daemon owns session PTY；GUI client owns attach command's local TTY/PTY；standalone shell owns its own local/SSH process”的三路契约。GPUI 不得为同一 Herdr session 再 spawn 第二个 daemon/session PTY，也不得把 attach client 的本地 PTY误删为“无 PTY”。

因此，原 P1-2 的修复要求更新为：删除“所有自有 PTY 都可能双 owner”的泛化说法，改为补充三路 owner/lifecycle 图，并分别验证 `herdr server`、attach client TUI、standalone shell 的退出、重连、`--takeover` 和窗口/输入转发语义。`alacritty_terminal` 只能作为 GPUI 本地 client/TUI 的解析或显示层候选，不能替代 daemon session owner 或 Herdr IPC。

### 其他项状态

- **P1-1、P1-3：未解决**。当前 planning 文件仍未出现可核验的 S1/S2 分离清单或 GPUI↔HerdrKit 跨语言决策。
- **P2-1：未解决**。已知 Ghostty pin 与未知 `herdr` CLI/server 发布 pin 仍未拆成表格。
- **P2-2：未解决**。当前仍未发现 `prototypes/index.html`；三版 HTML 尚不能验收。
- **P2-3：未解决**。`full-plan.md:21-41` 仍未加入来源与 scope 列。
- **GPT-6 Pro：未执行**，不得写成已通过。

当前审查结论：**未通过，仍有 P1 阻塞项；但 P1-2 已按真实源码收窄为 owner 边界问题，不再主张 GUI attach client PTY 本身是错误。**

## 最终复审（2026-09-18）

### 已验证

- `node planning/prototypes/check.mjs` 通过：`prototype structure/link/script check: ok`。
- `planning/full-plan.md:19-38` 已拆为 S1 主清单与 S2 条件附录；不再要求 S1 必须迁移 Hub 全业务。旧的 **P1-1** 结论已解决。
- `planning/full-plan.md:21-34,43-54` 已明确 Rust 协议 adapter、Herdr daemon/attach client/standalone 三路 owner，以及 Ghostty renderer pin 不等于 Herdr binary pin。旧的 **P1-2、P1-3** 结论已解决（此前报告中的同名初审条目由本节 supersede）。
- `planning/prototypes/index.html` 已存在；A/B/C 按钮、主题、⌘K、设置、split、断线模拟、选择行、attach/附件/通知等交互均为本地固定演示逻辑。文件未发现 `iframe`、网络请求、WebSocket 或外发 URL；原型明确标注“不连接 Herdr”。这是静态结构/逻辑检查，不是视觉或真实 Herdr 验收。
- `planning/chatgpt-6pro-prompt.md` 已创建，标注“未投递”；无绝对路径、私有仓库路径、密钥样式、账户/会话信息。其目标段仍同时描述 Hub 全能力与 S1/S2 候选，这是外部复核的比较摘要，不应当被当作已选 S1 实施清单。
- 所有 planning 内部 Markdown 链接均存在；`full-plan.md` 和 `README.md` 的原型、检查脚本、外部 prompt 链接均可解析。

### 剩余门禁 / 非阻塞项

1. **用户 scope 未确认**：当前文档推荐 S1，但用户尚未正式选择 S1/S2。
2. **默认布局未确认**：当前推荐 A，B/C 为同一能力模型变体，仍需用户确认。
3. **Zed/GPL 门禁未完成**：当前推荐原创 GPUI adapter、不直接嵌入/fork Zed `terminal_view`；最终 Cargo 图、依赖许可证与分发义务仍需实现前复核。
4. **GPT-6 Pro 未执行**：入口未证明、prompt 未投递，不能称为外部复核通过。
5. **X 评论缺口**：没有可读的具体 X 帖文/评论 URL，不能声称已验证社交推荐。
6. **运行时门禁未完成**：本轮未启动 Herdr/daemon，未做真实 attach、IME、VoiceOver、视觉、协议兼容或编译验收；原型 check 只证明静态边界与脚本逻辑。

### 需后续清理的过期文字

- `planning/review-findings.md:7-42,65-71` 保留了初审时的 P1/P2 记录，其中 P1-1/P1-3/P2-2/P2-3 的“未解决/不存在 HTML”判断已被本节 supersede；后续可将这些条目标记为历史初审，避免读者误把旧状态当当前结论。
- `planning/stack-research.md:5,40,63,70` 仍有“自有 PTY”泛化措辞；其 `2026-09-18 Herdr ownership correction` 已给出精确定义，但建议后续将开头结论统一为“协议 adapter + 按契约保留 client PTY”，避免脱离上下文阅读时误解。

**最终审查结论：规划文件与原型交付已通过本轮静态一致性复核；剩余为用户决策、许可证/构建和真实运行时验证门禁，不构成本轮 planning 阻塞。**

## 定向复审补充（2026-09-18）

- 原型已补第三个可见 pane、发送/Enter 模拟输出、通知列表、附件固定假文件名（明确不上传）、远程 attach 模拟状态与命令搜索；`node planning/prototypes/check.mjs` 通过。
- A/B/C 切换只修改 body class/标题，共用 DOM 与状态；pane 增删、模拟输出、连接状态跨布局保留，但无持久化。C 虽有三 pane，仍无拖拽比例、pane focus/键盘切换，不能当真实 split tree 验收。
- 所有模拟行为均有文本标识或 no-backend 语义；附件不是文件选择器，远程 attach 不是 Herdr attach。
- 剩余精确问题：无 `prefers-reduced-motion`；pane `×` 无 `aria-label`；`confirm()` 只属浏览器演示；check.mjs 不执行 JS、不覆盖事件挂载/状态/网络扫描。可由代理安全补强，但不阻塞原型方案。
- `external-review.md` 已有 3 条 X 帖/回复实读摘要；应表述为“已读体验线索，非工程事实”，不再写成“未读任何评论”。
- 当前仍需用户决定 S1/S2、A/B/C、GPL Zed路线或原创 adapter；GPT-6 Pro 仍未执行。尚无 Rust 工程/真实 Herdr runtime/视觉验收属于未来实施门禁，不是本轮方案交付失败。

## 最终 P2 复审补充（2026-09-18）

`node planning/prototypes/check.mjs` 输出 `prototype behavior/structure check: ok`。检查通过 Node `vm` 执行了布局类名、命令过滤和模拟输出函数，故不是单纯正则伪通过。

仍有两处精确逻辑问题，需由 prototype 代理修复或降级文案：

1. `#ratio` handler 设置 `--split`，但 CSS 没有消费该变量；range 实际不会改变 C 布局比例。应让 C pane 的 `flex-basis/grid-template-columns` 使用 `var(--split)`，或删去 range/注明静态演示。
2. `tabindex="0"` 只允许 Tab 聚焦，当前没有 focus handler 或 active-pane 状态；不能称 Tabfocus 会“激活 pane”。应绑定 focus 更新活动 pane，或改写为“可 Tab 聚焦”。

reduced-motion、pane labels、图标按钮名称、第三 pane、发送/Enter、命令过滤和模拟数据标注均已核实解决。剩余用户输入门禁及 GPT-6 Pro 状态不变；真实 Herdr/视觉/VoiceOver 不属于本轮原型方案失败。

## 最终 P2 关闭复核（2026-09-18）

`node planning/prototypes/check.mjs` 已通过，输出 `prototype behavior/structure check: ok`。新增 Node `vm` 断言实际执行 `layoutClass`、`filterCommands`、`appendOutput` 和 `setActivePane`，并检查 `--split` CSS 消费、range 写值、focusin、reduced-motion、pane labels 及无网络 API。

- C 的 `#panes` 现在使用 `grid-template-columns: var(--split,50%) calc((100% - var(--split,50%))/2) calc((100% - var(--split,50%))/2)`；range 改值确实改变第一列并平分余量。
- pane 的 `focusin` handler 调用 `setActivePane`，只切换 active class，不抢焦点；Tab 聚焦会更新活动 pane。

range 与 Tabfocus 两项 P2 已解决；本轮无需继续自行修改。剩余是用户 scope/布局、Zed 路线和 GPT-6 Pro 可用性门禁；真实 Herdr/视觉/VoiceOver 属未来实施验收。
