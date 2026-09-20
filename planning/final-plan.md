# Goose Herdr → Rust + GPUI 最终整合方案

2026-09-18｜本轮交付：研究、完整计划、三种选择原型与真实 6 Pro 外部复核。**不是原生应用实施，不代表用户已选型。** 本文为当前单入口；旧研究中的范围混写和历史状态以本文及最新审计为准。

## 1. 一页决策摘要

| 决策 | 推荐 | 仍保留的候选／边界 |
|---|---|---|
| 业务范围 | **S1**：完整保留真实 Goose Herdr 客户端业务；Hub 仅作架构与布局参考 | **S2**：S1 + Hub 全组件业务；必须在同一个 1-All 中完成，不是二期。用户尚未选择，不能用推荐 S1 偷减 S2 |
| 布局 | **A 高保真工作台** | B 终端专注、C 多任务分栏；同一业务模型，用户尚未选择默认 |
| 终端路线 | **TA：原创 alacritty_terminal + GPUI adapter**，优先验证 | **TB：直接复用／抽取 Zed terminal/terminal_view 源码**；正式候选，不是仅套皮，也未被批准或否决 |
| 保持不变 | Herdr daemon、CLI、socket、JSON-RPC、事件、attach 业务语义 | Rust 重写客户端协议适配，不直接链接 Swift HerdrKit，不增加第二个业务 daemon |
| 验收 | **按用户要求：编译成功即通过实现验收** | runtime、IME、VoiceOver、真实 Herdr、视觉、迁移另列已测／未测，不追加为用户签收条件，也不能冒称已验证 |
| 本轮状态 | 研究与方案交付完成；6 Pro 完整最终回答已采集 | 无 Rust 工程／Cargo 构建；没有真实 Herdr、IME、VoiceOver 或浏览器视觉验收 |

实施前只需用户决定范围 S1/S2、默认 A/B/C、技术 TA/TB（TB 同时涉及许可与分发选择）。二进制版本、源代码契约和构建环境由工具核实，不把可查事实变成用户问卷。未选不阻碍本轮报告交付；选型也不自动授权迁移真实数据或停止进程。

## 2. 证据及外部意见处理

- [Hub 能力扫描](capability-audit.md)：真实入口、组件、暂缓／预览／不可达分类。
- [Herdr 契约](herdr-contract.md)：真实业务目录 `/Users/eachann/Work/goose-herdr`；Hub 不是 Herdr 的业务源码。
- [技术研究](stack-research.md)、[GPUI 原生设计](native-design.md)：源码研究与视觉参考，非可编译实现证明。
- [外部复核记录](external-review.md)：X 公开主帖及回复已实读；体验评论不证明工程实现。官方源码查证另见技术研究。
- [6 Pro 完整原文](chatgpt-6pro-review.md)：输入区确切 `6 Pro`；最终页面显示思考 16m48s，停止按钮消失且出现复制／Pro反馈；仅一次脱敏提交，浏览器空间已关闭。执行证据来自浏览器代理记录，不是本整合环节重新操作浏览器。

### 对 6 Pro 的采用、纠偏与未采用

| 外部意见 | 整合处理 |
|---|---|
| S1 + A + TA | 保留为推荐，原文“默认采用”不等于用户已批准 |
| 三路 PTY、结果未知、提交后回滚保护 | 采用安全约束；按真实 Herdr 契约落地，不新增 wire 字段 |
| 唯一 PTY broker、能力注册表、多层 actor/controller、统一文件服务 | 是候选组织方式，不是现有事实或必建框架。先用既有函数／最小状态结构保证单 owner；只有真实复用需求再拆模块 |
| 原始消息层 + DTO | 保留字段、错误和转发数据的兼容要求；不强制每个只读响应都存两份，不建立无用途的持久原始消息库 |
| S1 也做 MCP、单写入协调、Hub mounted intent | 按来源纠偏：Hub MCP／envelope／writer queue／watcher／mounted intent 属 S2；S1 保留其真实 UserDefaults、Keychain 与客户端状态，不凭空增设 Hub 存储系统 |
| 每终端 4 MiB／总计64 MiB、50ms、100次循环、portable-pty 0.9.0 | 外部建议参数，未采纳为既有上限、选定依赖或用户验收条件；按实际基线确定，不添加跨平台 PTY 抽象“备用” |
| 全套 L0–L9、clippy/release/多架构作为必须验收 | 作为风险核验清单而非追加签收条件；用户编译成功即通过。安全数据操作仍须保护数据，支持声明仍需对应证据 |
| Tailcat 等同公开 tailscale/tailcat；新 MCP 规范 | 不作等同推断，不升级协议。实际 HerdrTailcat 使用本地 xcframework（见下），公开同名项目与新规范不证明兼容 |
| 接受兼容性例外、扩大 C 到非终端面板 | 不构成现有授权；不得据此删功能／新增业务。S2 非终端组件仅在选定 S2 后纳入 |
| IME／AccessKit／许可证“自动解决” | 不采纳任何保证；框架接口、manifest 与外部法律解释不等于本产品验证或最终分发结论 |

新增本地补查还发现 **Files 工作区**，已补入下表，不因外部摘要遗漏而丢失。6 Pro 原文保持不改。

## 3. 全部能力映射

下面是本轮已审计的具名能力全集，实施前仍需用源码调用图补齐任何新增发现，不能把“报告没写”当删除依据。来源的精确入口见所链接审计；每行均要在实现时关联落地入口，非新功能愿望清单。

### S1：真实 Goose Herdr

| 能力 | Rust/GPUI承载与保留点 | 证据 |
|---|---|---|
| 设备、workspace/space、agent session、分组、排序、状态、快捷切换 | 原身份与层级投影；拖拽失败恢复，持久化 owner 不改变 | Herdr `SidebarView.swift:48-220,552-670,1129-1210` |
| snapshot/catalog、pane 创建/读取/输入/关闭 | 协议兼容客户端；普通请求新连接与事件长连接分别保留 | `HerdrService.swift:140-145,299-333,534-714`；`SocketRPC.swift:38-40,134-160` |
| agent/terminal attach、takeover、ended overlay、显式 reconnect | 保留原 CLI 与参数，区分本地视图关闭／detach／服务端 pane.close | `HerdrService.swift:717-854`；`ContentView.swift:580-589,696-814` |
| 共享 daemon、GUI attach client PTY、standalone shell | 三路资源身份与退出策略分离，GUI 不杀共享 daemon | `LocalServer.swift:119-163`；`TerminalView.swift:270-305` |
| named session 与 socket 发现、最低 protocol version | 原路径、陈旧 socket 错误、ping/version 判定 | `Device.swift:1-18,50-55`；`HerdrSessionDiscovery.swift:40-61`；`HerdrService.swift:36-71` |
| local/SSH stream-local forward/Tailcat WireGuard、fallback | 原传输与信任策略；不绕过认证错误，不擅升 Tailcat | `Device.swift`、`HerdrService.swift`、`ContentView.swift:850-906` |
| split tree、比例、四向 focus/swap、原生菜单、快捷动作 | 递归树与键盘／菜单语义，布局切换不重 attach | `TerminalSplitTree.swift:1-151`；`GooseAgentApp.swift:130-170` |
| 终端字体/字号/行距/鼠标、可见性、遮挡与保活 | 保留偏好和活动对象；隐藏≠销毁；替代现有 Ghostty 的行为不得缩水 | `ContentView.swift:573-589,592-622,696-790` |
| Keychain token/SSH密码、附件上传、命令生成 | 保留标识、权限、取消、错误、quoting；秘密不写普通日志 | `HerdrService.swift:713-854`；`ContentView.swift:850-982` |
| Files 工作区、目录浏览/隐藏项/父目录/主页、文件传输/冲突策略/取消 | 真实本地/远端文件路径与SSH条件，上传/下载不误称仅附件 | `ContentView.swift:382,407,899`；`DeviceFilesView.swift:23-67,110-164,219-227,276-369` |
| retained spaces、选择/过滤、CLI路径偏好与其他现有设置 | 保留真实 UserDefaults/JSON 格式和默认行为，不套Hub存储格式 | `AppModel.swift:83-98,139-153,203-240,574-598` |

上述源码简称的绝对根：UI 为 `/Users/eachann/Work/goose-herdr/Sources/GooseAgent/`；HerdrKit 为 `/Users/eachann/Work/goose-herdr/Packages/HerdrKit/Sources/HerdrKit/`。本表是源码证据，不证明已连接实际 daemon。

### S2：S1 + Hub 全业务（同轮条件路径，不是二期）

| 能力 | 承载／不变项 |
|---|---|
| 商店浏览、详情、下载/校验/重试、安装 | 保留包格式、校验来源、错误；下载≠安装≠挂载 |
| 已安装列表、启停、卸载、懒加载main、显式打开、挂载/卸载 | 保留 `active = mounted && enabled`，禁用/卸载/删数据不混同 |
| mounted intent 与启动恢复 | 无旧字段不自动挂载；错误不覆盖用户意图；Run暂缓状态不偷偷取消 |
| 快捷键录入/冲突/注册注销、app-only/launcher | 保留活动组件 gate、冲突旧值、下次唤起生效与退出清理 |
| 命令面板、feature dispatch、原生菜单、组件菜单栏、窗口 | 不自动挂载；overlay关闭恢复组件焦点；单实例与macOS关闭隐藏按原宿主契约 |
| light/dark/system、首次旧数据导入 | 保留跳过/确认、导入字段、主题与焦点/可访问性语义 |
| 数据目录显示/迁移/采用新空目录/恢复备份 | 原白名单、路径重叠拒绝、schema、envelope、队列、watcher、失败保存与停止顺序 |
| 外部MCP、SDK IPC | 保留Unix socket/stdio、工具schema、活动gate、大小/连接/并发/超时；storage/clipboard/shell/notification/subInput/lifecycle/focus/visibility；不自建新版HTTP后台 |
| 2FA | OTP/TOTP、账户/分组/排序、快速取码粘贴、剪贴板导入、屏幕二维码、多格式导入、删除/回收站、迁移、安全存储、MCP vault；不得以普通JSON替代秘密保护 |
| Marks | CRUD、分组排序、模板URL、搜索/回车打开、导入导出、图标缓存/回退、死链、附件、同步安全、MCP；现有Responses/兼容Chat/Anthropic接入保持能力与凭据边界 |
| Monitor | 应用/Helper归并、GPU/标签/网络服务、端口搜索/整组终止、窗口分类、网络速率、providers、PID复用二次校验、偏好、MCP、owned-processes |
| Run | 脚本CRUD/分组/搜索、运行/停止、cwd/env/shell/确认、真实PTY、resize/粘贴/IME、多会话、scripts.json/window.json；`scriptId/free`与8上限仅属于Run |
| 不可达/受限项 | Hub中Run仍暂缓，独立GooseRun不能冒称已集成；uTools的current-browser/内建AI/profile无真实桥接不伪造；预览fixture不算运行能力 |

来源：[capability-audit.md](capability-audit.md) 第2–5节，已列每项真实文件。S2实施必须逐项解决原组件的执行宿主/包兼容：Rust UI不能直接当Electron/JS/native addon运行时。可用真实兼容承载或等行为迁移；未经用户同意不能把不能运行的组件换成空页面。是否提升Run的暂缓状态需明确决策，不因S2自动推断其当前可达。

### 两范围共有的终端与原生交互

保留已选范围的实际行为：shell/env/cwd、输入校验/尺寸、SIGINT与Ctrl-C、EOF与Ctrl-D、VT/ANSI/UTF-8/CJK/emoji、颜色/宽度、scrollback、选择/复制/粘贴/搜索、alternate screen、鼠标/bracketed paste、OSC/bell/光标、重启/退出码。按原终端支持集建立对照，不擅自扩展协议。

GPUI承载：后台顺序读取解析、按帧/脏区绘制；焦点/滚轮/选区、IME marked text/commit、剪贴板/上下文菜单、窄窗/DPI、可见性。保持键盘焦点反馈、名称/角色/状态、reduced motion；终端已提交的命令不能伪装成编辑框撤销。OSC副作用经过既有安全策略，不因重绘重放。

## 4. 最小架构与三路所有权

```text
GPUI窗口/树/分栏/动作 ── Rust Herdr客户端 ── 原local/SSH/Tailcat ── 共享daemon
     │                                                     └─ 业务PTY（daemon所有）
     ├─ GUI attach client PTY ── 原herdr attach命令 ────────────┘
     └─ standalone shell PTY（GUI按旧策略管理）
输出字节 → 终端解析/状态 → GPUI绘制；输入/resize → 对应原owner
```

- Rust不直接链接SwiftKit；协议客户端保留newline JSON-RPC、事件订阅、socket路径、协议版本、错误和attach参数。不能自创revision/cursor、后台服务、租约或新daemon方法。
- 业务session、attach实例、视图不能共用一个含混“session owner”；只需保证资源身份与释放边界，不要求专门broker框架。
- 隐藏/布局切换不终止业务PTY；GUI退出仅清理自有资源。attach退出、控制通道断线、服务端结束是不同状态；依原权威结果决定ended。
- 非幂等请求发送后失联显示“结果未知”，不自动重发、不假称取消远端。旧异步回调不得覆盖新连接状态；背压不丢未解析字节，不用无限队列拖死其他会话。
- Herdr本机原有按需启动能力保留，但本轮不执行。`LocalServer.swift:119-125`明确共享daemon不随GUI退出终止。

## 5. TA／TB与依赖基线

| 项目 | TA | TB |
|---|---|---|
| 实现来源 | 原创GPUI输入/绘制/宿主适配，复用选定终端解析库 | 实际抽取Zed源码，或保留必要依赖闭包后嵌入；不是直接可用公共widget |
| 成本 | IME、选区、搜索、鼠标、a11y等需自行实现 | workspace/editor/project/db/settings等耦合、Drop/spawn/kill路径、上游补丁维护 |
| 所有权 | 不启用第二套PTY event-loop；仅接既有client/standalone边界 | 必须替换默认任务/子进程生命周期，绝不可让视图析构杀共享会话 |
| 许可 | 不复制GPL实现；实际锁定组件仍逐项审查 | GPL代码与依赖/资产逐文件登记，分发义务未明确前不分发；包装层不消除许可责任 |
| 维护 | 固定GPUI/终端依赖，升级跑兼容检查 | 同一Zed commit固定闭包、原fork revision、来源与适配补丁；按commit diff同步 |

**研究快照，不是本地构建结论：** 先前官方源码研究记录 Zed `94c997e06faabe6538e608c6d04091914f02f86b`，该manifest的GPUI `0.2.2`/Apache-2.0，terminal/terminal_view `0.1.0`/GPL-3.0-or-later。固定commit来源在 [stack-research.md](stack-research.md)；本整合未重新联网验证，不称其为最新或已兼容版本。

6 Pro提到alacritty_terminal `0.26.0`、portable-pty `0.9.0`和Zed的fork，属于外部检索候选，不是已安装/锁定依赖；其main/latest链接不能替代最终固定版本。gpui-kit的公开重定向同样仅为研究记录，最终以实际manifest确认，不因可用就必须加组件库。

现有Ghostty `1.6.20260909`／revision `7e45d27160f9b34aca9ca5c9820e9207482f9f04`属于旧renderer依赖，绝不是Herdr binary pin（来源见Herdr契约）。HerdrTailcat的本地二进制目标见 `/Users/eachann/Work/goose-herdr/Packages/HerdrTailcat/Package.swift:14-19`，上游身份与hash还待核验。

实施时记录Rust toolchain、GPUI精确commit、终端库版本/来源/checksum、必要PTY依赖、Cargo.lock/features、SDK/Xcode/macOS/架构、Herdr CLI版本/hash/协议、Tailcat制品来源/hash与LICENSE/NOTICE。当前目标无Cargo.toml；真实Herdr版本、整套依赖兼容性和分发结论均未pin/未验证。不要为了填表声称已编译。

## 6. 全部1-All步骤（选定范围内一次闭环）

| 步骤 | 依赖 | 执行与产物 |
|---|---|---|
| 1 | 无 | 记录用户S1/S2、A/B/C、TA/TB选择；冻结Hub、真实Herdr和目标状态，不覆盖源工作树；现有能力逐项定来源 |
| 2 | 1 | 核实实际Herdr CLI/daemon构建、版本/hash、协议/错误/超时、启动与三路owner；只在受管环境且获准时调用Herdr，不能伪设HERDR_ENV |
| 3 | 2 | 提取客户端字段、设置/Keychain、文件与上传、传输、split/菜单的兼容样本；S2加包/组件/SDK/MCP/数据/停止顺序；合成数据不含秘密 |
| 4 | 2–3 | 对选定终端路线做最小构建/所有权适配；若两路仍需比较，用隔离小验证，不强制两套产品；锁定依赖及许可来源 |
| 5 | 4 | 最小Rust/GPUI应用壳、窗口/主题/动作/焦点；复用已有库，不先造插件框架或跨平台抽象 |
| 6 | 3–5 | Rust Herdr协议客户端：local/SSH/Tailcat、版本检查、普通RPC/事件、snapshot、错误、重连/结果未知；不改daemon协议 |
| 7 | 4–6 | attach client和standalone管理、字节流/resize/退出；终端解析/绘制/IME/选择/鼠标/剪贴板/OSC；保留三路ownership |
| 8 | 6–7 | 完成S1表全部业务：导航/分组排序、catalog/pane/attach/takeover/ended/reconnect、split/focus/swap、菜单/快捷动作、偏好、Keychain、附件、Files和命令生成 |
| 9 | 3、5；S2 | 完成Hub商店/包校验/组件宿主、安装/启停/卸载/挂载/恢复、快捷键/窗口模式/菜单/主题/首次导入；处理真实JS/native addon兼容而非占位 |
| 10 | 9；S2 | 完成2FA、Marks、Monitor、Run表每项与数据兼容；明确Run暂缓/独立状态的目标，保留uTools能力边界 |
| 11 | 9–10；S2 | 接完整MCP/SDK IPC及活动gate、权限/限制、生命周期/停止保存；不新增协议版本或常驻进程 |
| 12 | 8；S2加9–11 | 实现所选默认及A/B/C等能力布局、命令/焦点/窄窗/可访问性，切布局不改session/attach/挂载意图；原型不是原生实现代码 |
| 13 | 3、8、12；S2加9–11 | 按下节实际数据类别建立备份/预检/安全切换/恢复；迁移真实数据前核实Keychain可读和写入者；不搬共享daemon目录 |
| 14 | 全部相关实现 | 编译已选目标；按用户规则编译成功即实现验收通过。报告能力实现位置和未测风险，失败不报成功；非平凡逻辑保留最小可运行检查 |
| 15 | 发布被请求时 | 处理签名/打包、许可证/NOTICE、升级/回退与权限支持声明；声明必须有对应证据，不能把“编译验收通过”写成全面runtime认证 |
| All | 全部相关项 | 选定范围矩阵无默默删除；提交完整变更/构建结果/已知未测项/数据恢复说明。S2不是后续二期；未选S2不声称Hub全量迁移 |

所有步骤均已规划，无“先删以后补”。发布工作是完整生命周期清单，非本轮授权执行；原型完成也不触发上述实施。

## 7. 三版入口与流程

打开 [三版交互原型](prototypes/index.html)，顶部 **A／B／C** 即三个入口（同一HTML，无伪造直达hash）：

- **A（推荐）**：对象树 + 中央终端 + 按需详情，管理与操作可发现。
- **B**：终端专注，侧栏/详情隐藏但共用模型与命令入口，不删除业务。
- **C**：多pane展示、比例range、Tab活动pane；是布局/流程模拟，不是原生递归split tree验收。

原型已有发送/Enter模拟、断线/重连、split、附件/通知/远程连接演示、命令过滤、主题、焦点与reduced-motion；数据固定无网络，附件不上传。原型的通知演示不自动升级为新增S1通知中心需求。

同页内联SVG与文字流程：

```text
S1：设备/传输 → 原daemon控制通道 → snapshot/事件 → 已有pane
    → GUI attach client → GPUI终端 → split/输入/附件/Files/偏好
    → 断线辨别/显式重连 → 清理自有client（daemon继续运行）
S2：首次导入 → 商店/安装校验 → 启用/显式挂载 → 四组件
    → 命令/菜单/MCP → 停止保存/恢复 → 备份/迁移/安全回退
```

S1交互原型已做；S2流程在此完整规划，**并未声称有S2全业务可运行HTML原型**。原型范围未选不能被解释成删减S2。GPUI视觉按[原生设计](native-design.md)：Delta参考的扁平表面、细分隔、中性选中、克制强调，复用语义token；保留键盘focus-visible等价反馈，不搬Web重卡片体系。截图中的字号是参考不是工厂默认。

## 8. 数据迁移、停止与回滚

**先分清数据，不为了迁移而迁移。** S1保留真实UserDefaults/JSON、Keychain引用与客户端偏好；没有数据根迁移需要就沿用兼容读取，不强建journal/全局writer框架。S2才逐项保留Hub数据目录白名单、JSON envelope/写队列/watcher、mounted intent与原迁移API。新GUI偏好不得破坏旧业务schema。

确有数据迁移时：预检来源/目标/容量/权限/符号链接/重叠 → 按原顺序停止所有相关**本地写入者** → 一致备份 → 目标staging白名单复制 → schema/字段/校验/Keychain读回 → 可恢复切换 → 只读健康检查 → 明确提交后恢复写入。普通目录复制不迁移Keychain访问权；不得删建同名秘密假装成功。空目录新建与已有数据迁移是不同动作。

共享daemon、远端数据/socket/PID不属于GUI迁移白名单；不杀daemon解决写入冲突。新锁不保证旧程序遵守，必须确认旧写入者退出；不要未经验证把两次rename当原子事务。

- **切换前失败**：保留原根；只清理明确属于本次操作的staging，未知路径不删。
- **切换后未开放写入**：健康检查失败可恢复原指针/根，保留诊断。
- **提交并产生新数据后**：先停写并完整保存新数据；确认旧版能读或有安全反向转换，再回退。**绝不拿旧备份覆盖新写入。** 无损回退无证据则停在可恢复状态，请求处理，不报成功。
- **进程/协议回退**：只断开本客户端、保留共享session，用旧客户端重新连接；未知副作用查询实际状态，不重放、不重建替代会话。
- **S2包回退**：恢复兼容旧包/安装记录/挂载意图，不把卸载与删除用户数据混同；保存失败保留writer/队列，不能宣布已清理完成。

## 9. 风险与核验边界

| 风险 | 处理要求／仍未核验 |
|---|---|
| 双spawn/误杀 | 审计构造/Drop/close/attach失败路径；三路资源分离。本轮未观察真实进程 |
| 协议/恢复不兼容 | 保留字段/错误/顺序/版本，分片与未知结果；无server cursor不自创。实际兼容测试未执行 |
| 终端成熟度/IME/VoiceOver | TA须重建行为，TB也须产品实测；marked text不入PTY、commit一次、稳定语义与焦点；全部未验证 |
| 许可/依赖漂移 | 最终版本/文件/资产/分发模式审查，固定commit/lock；本轮研究不是法律结论或授权 |
| 数据/Keychain/回退 | 备份、实际读回与提交边界；新数据优先保护；签名访问尚未验证 |
| 性能/慢消费者 | 按原支持规模定预算，后台解析/合并绘制，不丢字节/无限增长；没有基准结果 |
| S2安全 | 包校验/路径安全、OTP秘密、同步边界、PID二次核验、MCP权限/取消/日志；不因换UI弱化 |
| 范围漂移 | Hub预览/Run暂缓/uTools-only不伪造；S1不冒称Hub全迁移；macOS迁移不推断删除现有iOS产品或新增Linux/Windows目标 |

所有安全限制用于避免数据损失和越权，不代替用户批准新功能。未测风险保留在实现报告；**用户的编译验收标准不被外部模型改写**。

## 10. 本轮原目标闭环与验证

| 原目标 | 完成证据／状态 |
|---|---|
| Hub架构/布局/具名能力扫描 | [capability-audit.md](capability-audit.md)，已完成文档扫描 |
| 真实Herdr契约、底层不变 | [herdr-contract.md](herdr-contract.md) + 本文三路owner/Files补查；版本pin仍属实施事实核验 |
| X帖子/评论与官方技术查证 | [external-review.md](external-review.md)末节3组实读线索 + [stack-research.md](stack-research.md)官方源码；社交意见不作工程证据 |
| GPUI原生风格 | [native-design.md](native-design.md)参考图/令牌/交互边界；无视觉运行验收冒称 |
| 全部步骤/映射/迁移/回滚/风险/两路线 | 本文第3–9节；S1/S2均有完整同轮条件路径 |
| 三版原型、A推荐、流程图 | [prototypes/index.html](prototypes/index.html)顶部A/B/C与内联SVG；S2条件流程见第7节 |
| 真实6 Pro完整复核 | [chatgpt-6pro-review.md](chatgpt-6pro-review.md)完整9节/1-All原文；[external-review.md](external-review.md)成功与关闭证据 |
| 本地验证 | `node planning/prototypes/check.mjs`及Markdown本地文件链接检查；结果见当前完成审计 |

**结论：本轮研究、方案与选择原型的必交项已齐。** 尚未选择的范围/布局/技术路线是待用户设计决定，不是研究报告未完成；尚无Rust编译/真实运行是本轮未实施边界，不是虚假的通过项。
