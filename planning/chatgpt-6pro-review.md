# ChatGPT 6 Pro 外部复核原文

- 核验日期：2026-09-18
- 页面思考强度：`6 Pro`（非近似模型替代）。
- 对话：https://chatgpt.com/c/6aad0012-0a0c-83e8-ad0e-8d81a9332426
- 完成证据：页面显示“思考了 16m 48s”；停止回答按钮消失，出现“回复操作 / 复制回复 / Pro 反馈”；最终正文 25686 字符。
- 采集时间：2026-09-18T09:28:09.373Z
- 下文为最终 assistant 正文原文（DOM innerText），不含思考过程、用户输入、账户或侧栏。表格采用页面制表符排版；文内来源链接附后。模型建议不是项目事实或用户批准。

---

Goose Hub → Rust + GPUI：最终架构复核与 1-All 实施方案

单一架构推荐：S1 + A 高保真工作台。终端优先验证 TA：原创 alacritty_terminal + GPUI adapter；TB：直接复用／抽取 Zed terminal 源码仍保留为正式候选，不视为已经否决或批准。

这里有一个必须保留的边界：S1 不等于“Goose Hub 全业务迁移完成”。 如果最终批准的是 Hub 全业务与 Herdr 合并，交付范围就是 S2，本文列出的全部 S2 项必须在同一个 1-All 计划内完成，不能转成“二期”，也不能因为推荐 S1 而被删除。

本次没有私有源码、协议样本、旧数据和真实运行环境，因此以下内容分为三类：

标记	含义
摘要基线	你提供的既有能力与约束，作为必须保留的需求；不代表已经通过源码或运行时核验。
外部事实	截至 2026 年 9 月 18 日检索到的官方文档与公开源码信息，附引用。
实施规定／待核验	本方案提出的架构、执行顺序和验收规则；实际行为必须回到项目源码与运行时确认。

本文覆盖摘要中的全部范围；不声称已经覆盖未披露的源码功能。 首轮审计发现的额外既有能力，应补入同一能力矩阵和同次验收，不得默认丢弃。

1. 范围、默认原型与终端路线
1.1 S1 与 S2：单一推荐，但不代替范围批准
比较项	S1：保留 Herdr，借鉴 Hub 壳架构	S2：Hub 全业务与 S1 合并
业务边界	Herdr 全部客户端能力；Hub 提供布局、命令入口、生命周期等架构参考	S1 加商店、组件管理、2FA、Marks、Monitor、Run、数据迁移及全部 MCP 适配
Herdr 边界	daemon、CLI、socket、协议语义与业务 PTY 保持不变	完全相同；合并产品不能顺带重构 Herdr 服务端契约
主要迁移工作	客户端协议适配、连接管理、三路 PTY、终端、分栏、原生交互	S1 全部工作，另加模块宿主、包兼容、四组件业务、旧数据及跨模块生命周期
最大风险	attach 生命周期被终端后端接管；协议和状态恢复不兼容	S1 风险，再叠加跨模块写入、挂载恢复、旧脚本与包格式、秘密数据迁移
迁移成本判断	集成面较集中，但终端和真实 Herdr 验收仍是重工作	明显更高，且增加的是交叉集成成本，不只是多写几个页面
能否宣称 Hub 全量迁移	不能	仅在全部 S2 能力验收通过后可以
本方案推荐	推荐 S1	完整保留为同轮条件范围

推荐 S1 的原因不是“Hub 能力不重要”，而是Herdr 的协议兼容与所有权安全已经构成独立、完整的迁移目标。把商店、秘密存储、进程控制与脚本宿主一起合并，会让终端问题、生命周期问题和存储问题更难隔离。

**范围未批准时的默认处理：**允许继续做契约审计、公共基础设计和隔离原型；禁止据此裁剪 S2、执行真实数据迁移，或发布“全量迁移完成”的结论。

1.2 默认采用 A；B、C 是完整能力的布局变体

默认选 A 高保真工作台：设备、workspace、space、session 的层级关系和连接状态保持可见，适合这个项目同时包含“管理对象”和“操作终端”的特点。

B 终端专注、C 多任务分栏不创建独立业务模型，也不拥有独立 session 管理器。三者共享：

同一个能力注册表、同一个命令系统、同一套 session 引用、同一个递归 split tree，以及同一组权限和错误状态。

切换布局只能改变展示偏好，不能触发重新 attach、重新 spawn、关闭 server session，或把未显示面板当成已卸载组件。

1.3 两条终端路线完整比较

为避免和布局 A/B 混淆，下文用 TA／TB 指代终端技术路线。

公开源码已经确认：Zed 的 terminal 与 terminal_view 包均标注 GPL-3.0-or-later；terminal_view 依赖 editor、project、workspace、db、settings 等内部模块。GPUI 自身包标注的是 Apache-2.0，不能把 GPUI 的许可证推导成整个 Zed terminal 的许可证。
GitHub
+2
GitHub
+2

维度	TA：原创 alacritty_terminal + GPUI adapter	TB：直接复用／抽取 Zed terminal／terminal_view
真实复用范围	复用终端解析、网格和相关基础能力；输入、绘制、宿主行为、可访问性及生命周期由项目实现	实际复用 Zed 的终端模型、视图与相应实现代码，不是只参考视觉
产品控制权	容易将业务 session、PTY 和 renderer 解耦	必须主动拆除或替换 Zed workspace、项目、任务与持久化假设
初始工作量	adapter 工作更多，尤其是 IME、选择、搜索、鼠标和 a11y	可复用既有实现，但依赖闭包和行为改造工作不可忽略
PTY 所有权	可以从接口层禁止 renderer spawn／kill	必须审计构造、关闭、析构、任务结束等全部路径
许可证	GPUI、上游 Alacritty 的包许可证为 Apache-2.0；仍需审计实际依赖与文件	需要接受并落实 GPL 等实际适用的分发义务，或取得其他有效授权
固定版本	独立锁定 GPUI、终端库、PTY 后端和 transitive dependencies	同一 Zed commit 固定整个相关闭包，同时保留其终端库 fork 的精确 revision
上游同步	分别升级 GPUI 和终端库，主要维护 adapter	维护抽取补丁、内部接口替换、许可证清单及上游行为变化
可访问性	项目承担完整语义树和运行时验证	可利用已有接口与实现，但不能继承“已经通过本产品 VoiceOver”的结论
推荐状态	优先验证候选	正式备选，必须完成同标准验证

上游 alacritty_terminal 自身也包含 PTY／event-loop 模块，并非只有解析器；因此选 TA 也不自动保证没有双 owner。方案要求只由指定的 PTY broker 使用一个后端，不能同时启动 portable-pty 和 Alacritty 的 PTY 事件循环。
Docs.rs

更直接的风险证据是：本次检索到的 Zed Terminal::drop 包含子进程终止、PTY shutdown 和进程终止调度逻辑。TB 不能原样把这个对象的生命周期绑定到 Herdr 的共享业务 session。
GitHub

TB 的两种实际实施方式都纳入验证：

TB 子方式	必须完成的工作	不可接受的替代
保留依赖闭包后嵌入	记录所有内部 crate、功能开关、资产、许可证；把 Zed 生命周期映射到本产品明确的 owner	把整个 workspace 带进来后，以“能编译”代替边界审计
抽取源码并替换内部接口	为主题、命令、项目上下文、持久化和 PTY 注入建立接口；逐项维护补丁和来源	只复制绘制代码，却声称已复用完整 Zed terminal

GPL 的分发义务不能靠增加一个 permissive wrapper 或改变目录结构自动消除；具体发布方式必须经过许可证审查。许可证未批准时，TB 可以继续做隔离研究，但不得把相关代码混入 TA 的正式实现。
GNU
+1

**路线决策规则：**两路使用同一套终端语料、attach 场景、IME、VoiceOver 和性能测试。只有在许可证、所有权、兼容性均通过后才比较维护成本。TA 通过则默认选 TA；TA 未通过而 TB 全部通过，可以选 TB；两路均未通过则阻塞发布，不降低门禁。

1.4 必须生成的版本锁定清单
对象	锁定要求	当前状态与默认处理
GPUI	精确 40 字符 commit；相关 Zed crate 来自一致源码基线	未定。不能把不同时间抓取的 main 文件拼成一套“已兼容版本”
Rust toolchain	精确版本、components、targets；与选定 GPUI 基线一起验证	未定。读取同一 commit 的要求后固定，不使用浮动 stable
TA 终端库	精确 crate 版本或 commit，保留 checksum／来源	检索到的文档版本为 0.26.0，可作为验证候选，不代表项目已选定。
Docs.rs

TA PTY 后端	建议首先验证 portable-pty = 0.9.0；若不满足，替换候选必须重新过 owner 测试	该版本文档提供 master/slave、spawn、read/write 等接口，但本项目兼容性未验证。
Docs.rs

TB 终端依赖	固定 Zed commit 及其实际终端库 fork revision	Zed workspace 使用自己的 Alacritty fork；不能静默换成同名 crates.io 包。
GitHub

Herdr binary／CLI	保留既有部署与兼容策略；记录版本和二进制 hash	独立于 renderer 版本，不因 GPUI／Alacritty 升级而替换
Tailcat／辅助程序	固定实际使用版本、构建来源及 hash；系统 SSH 则记录支持的系统范围	不以公开项目“最新版本”替代既有版本
发布依赖	Cargo.lock、合法 feature 组合、SDK／Xcode、最低 macOS、SBOM、LICENSE、NOTICE	全部进入构建产物清单；缺项不能进入发布门禁
2. 按依赖排序的 1-All 实施计划

以下是实施时的完整顺序，本轮不执行代码改动。条件范围 S2 不代表后续阶段，而是范围批准后必须纳入同一次完成条件。

编号	依赖	执行内容	必须交付的证据／产物
01	无	冻结范围与现状；审计 Herdr 构建入口、锁定依赖、协议、framing、进程、IPC、权限、错误与许可证	范围 ADR；构建入口图；契约目录；三路 owner 图；未决项登记
02	01	建立旧实现行为基线，覆盖成功、失败、超时、断线、恢复和边界输入	脱敏黄金样本；旧客户端操作录像；协议序列；数据 fixture；错误对照表
03	01–02	审计存储、mounted intent、停止顺序、迁移白名单、Keychain 与签名身份	存储字段清单；停止依赖图；迁移清单；秘密数据流图
04	01–03	对 TA、TB 分别做隔离技术验证；固定依赖；完成许可证闭包审查	两路线可重复构建说明、关键终端运行结果、依赖与补丁清单
05	04	以同一标准选择一条正式终端路线	路线 ADR；未通过项；明确的选定版本；禁止双后端 owner 的接口约束
06	02–05	建立 Rust 应用基础：命令路由、单写入协调、错误封装、权限、原生服务接口	模块边界测试；无业务副作用的 GPUI 壳；统一命令注册表
07	06	重写 HerdrKit 客户端适配：framing、DTO／raw 保真、连接、订阅、恢复	与旧客户端的协议差分测试；原始错误保真测试；断线状态机
08	05–07	建立 PTY broker、attach controller、standalone controller、终端解析与绘制管线	单 spawn／单 owner 测试；真实 PTY；attach 退出不误杀 server session 的证据
09	07–08	完成全部 S1 功能：树、分组、catalog、snapshot、pane、split、takeover、上传及偏好	S1 能力矩阵每行对应实现入口与自动／人工测试
10	03、06、08；S2 批准	完成 Hub 壳业务：商店、包校验、安装、启停、挂载、恢复、快捷键与窗口模式	包／宿主兼容报告；安装失败恢复；mounted intent 与停止顺序测试
11	10；S2 批准	完成 2FA、Marks、Monitor、Run 全部业务与旧数据兼容	四组件矩阵全部测试；秘密、同步、PID、脚本和窗口数据对照
12	06、09；S2 时加 10–11	完成 A/B/C 三种布局、命令面板、原生菜单、键盘与 a11y 语义层	三原型交互规格与运行原型；布局切换不改变业务对象的证明
13	06；S2 时加 10–11	完成 MCP 适配与宿主生命周期，连接统一业务服务	真实 MCP 客户端结果；工具 schema／错误对照；权限及取消测试
14	03、09、12；S2 时加 10–13	实现并演练迁移事务：预检、冻结、备份、staging、校验、切换、回滚	每个事务边界的故障注入报告；旧版本重开验证；Keychain 可读验证
15	所有选定范围实现完成	完成性能、安全、视觉、IME、VoiceOver、真实 Herdr 和升级／降级验收	分层验收报告；问题关闭记录；最终版本和许可证产物
All	01–15 全部相关项通过	签名发布候选、完整升级／回退演练和范围签收	能力矩阵无未处置缺项；发布清单；可执行回退手册

可并行的是已经冻结接口后的协议、UI、组件和测试工作；不能并行争抢同一契约、同一数据写入入口或同一 PTY owner。 04 的两路线验证也只能各自在隔离进程和测试 session 中运行。

3. 责任边界与关键时序
3.1 模块责任

以下模块名是建议边界，不假定现有仓库已经存在这些目录。

模块	负责	明确不负责
app / command_router	窗口、布局、焦点、命令可用性、确认和结果展示	直接拼 RPC、直接 kill、直接改业务数据文件
herdr_protocol	既有请求／响应／事件、framing、字段与错误保真	自创 daemon 方法、修改服务端 schema
herdr_connection	原传输拓扑、认证、订阅、重连与一致性恢复	把连接断开判成业务 session 结束
session_controller	attach／takeover／reconnect 的状态与意图	自行拥有共享业务 PTY
pty_broker	GUI 所有 PTY 的唯一创建、输入、resize、等待与关闭入口	终止未确认属于自身的进程或共享 daemon
terminal_core	有序解析字节流、网格、模式、scrollback、搜索与快照	创建 shell、访问 Keychain、直接操作系统剪贴板
terminal_gpui	绘制、选择、输入法、命中测试、焦点、a11y 语义	因视图析构而销毁业务 session
storage_compat / migration	原格式读写、单写入、字段保真、事务迁移与恢复	把 daemon 数据目录当 GUI 目录搬迁
native_macos	Keychain、原生菜单、快捷键、剪贴板、系统权限、窗口平台接口	自行绕过业务权限或直接修改组件状态
module_services，S2	Hub 与四组件业务、挂载依赖和恢复	各自建立相互不知情的存储写入器
mcp_adapter，按既有范围	原 MCP 契约与统一命令／业务服务之间的映射	第二套业务实现、第二个数据 owner、自动扩大工具权限
3.2 三路 PTY：必须用类型和资源标识区分

至少区分以下内部标识，不能全部叫 session_id：

server_session_id   —— Herdr 服务端业务会话标识
attach_instance_id  —— 本次客户端 attach 实例
terminal_view_id    —— 一个 GUI 展示对象
run_slot_id         —— Hub Run 的既有 session/free 配额对象
generation         —— 本客户端一次连接或运行世代，仅供内部隔离旧回调

这些内部标识不能擅自变成新的 Herdr wire 字段。

类型	创建和最终所有者	GUI 可以做什么	GUI 退出时	“最多 8 / session/free”
共享 daemon 的业务 PTY	既有 Herdr server／daemon	按原协议查看、attach、takeover、显式执行已存在的 session 操作	不得因 GUI 退出而终止	不得套用 Run 规则
attach client PTY	GUI 的唯一 PTY broker，或审计确认的既有客户端宿主	启动一个 attach 客户端；输入、resize、detach、等待其退出	仅清理自身 attach 资源；必须验证不会间接关闭 server session	不得默认计入 Run 配额
standalone shell PTY	GUI 的 standalone controller／broker	启动与管理自身 shell	按旧 standalone 策略关闭、确认或保留；不能擅改	只有属于 Run 的对象才按 Run 契约计数

内部资源释放可做幂等，但对外 free 的重复调用返回值必须保持旧行为。 不能把内部 Drop 的幂等性强行变成新的 API 语义。

3.3 启动与 attach 时序
用户 → GUI：打开已有 Herdr session
GUI → session_controller：OpenExisting(server_session_id)

session_controller → herdr_connection：
    按既有拓扑建立控制通道、认证、读取状态、订阅事件

herdr_connection → 既有 daemon：
    发送原有请求，不引入新方法或新 framing

session_controller：
    核验当前 session 状态、attach 权限、是否需要 takeover
    为本次 attach 分配独立 attach_instance_id / generation

session_controller → pty_broker：
    仅创建一次 attach client PTY，运行审计确认的原 attach 命令
    [原实现不经过该方式时，照原拓扑实现，不额外包一层 PTY]

attach client ↔ 既有 daemon ↔ 已有业务 PTY

attach 输出 → terminal_core：
    按顺序解析，不经过 GUI 文本重编码

terminal_core → terminal_gpui：
    网格快照、模式和脏区通知

只有原协议／原客户端可验证的 attach 成功条件成立：
    GUI 才显示 Attached

不能用“attach 子进程已 spawn”代替“已连接到目标 session”。

3.4 关闭、停止与退出时序
关闭视图：
    释放 terminal_view 引用
    → 是否 detach 取决于明确的原产品行为
    → 不自动发送结束 server session 的请求

断开 attach：
    禁止新输入
    → 按原 detach 机制结束 attach client
    → 等待本地资源退出
    → 服务端 session 状态继续由 Herdr 确认

显式结束 server session：
    用户明确动作／确认
    → 调用既有 Herdr 业务方法
    → 等待原协议结果或进入“结果未知”
    → 不把本地 attach 退出当成服务端操作成功

退出 GUI：
    阻止新命令
    → 按已审计停止顺序停止本地写入者
    → 处理自身 standalone shell
    → 断开自身 attach
    → 注销 GUI 所有快捷键、关闭本地资源
    → 共享 Herdr daemon 保持运行

“关闭 pane”需要特别审计。 原 Herdr pane 操作可能具有服务端业务含义，不能把它和“隐藏 GUI 面板”合成一个动作。

3.5 连接状态与业务状态必须独立

建议内部采用两个状态机：

连接：
Disconnected → Connecting → Authenticating → Synchronizing → Live
                            ↘ Failed / RetryWait

attach：
Detached → Attaching → Attached
                       ↘ TakeoverConflict
                       ↘ DisconnectedUnknown
                       ↘ EndedConfirmed

关键规则：

EOF、网络中断、attach 子进程退出，不足以证明 server session ended。

ended overlay 仅由原契约允许的权威证据触发；状态未知时显示“连接已断开，服务端状态待确认”。

原实现允许自动恢复的控制通道沿用原策略；显式 reconnect 的交互语义不能被自动重建 session 替代。

每次重连增加内部 generation，旧连接的异步结果不得覆盖新状态。

takeover 只能使用原有服务端机制。客户端互斥锁不等于跨客户端独占权，不能假装提供服务端没有的 CAS／租约保证。

3.6 协议、错误、超时和 backpressure

协议保真。 建立“原始消息保留层 + 类型化访问层”。必须保持字段名称、类型、缺失／null 区别、ID、时间单位、枚举、排序语义、数字精度及错误的 code/message/data。未识别字段不得因为反序列化再序列化而消失。

framing。 按摘要保留 newline JSON-RPC，但以下细节必须由源码和样本确定：LF／CRLF、空行、最大帧、分片 UTF-8、尾部无换行、非法 JSON、stdout／stderr 分离、并发响应与事件穿插。不能把 read() 一次返回当成一条消息。

超时。 既有连接、请求、attach、上传、停止超时逐项提取并沿用。原型可增加“长时间等待”的提示，但不能偷偷把它变成新的协议超时或远端取消。

操作结果未知。 非幂等请求发送后失联，默认不自动重发；先用原协议能够提供的状态查询核对。无法核对则展示结果未知，不能报成功，也不能断言远端未执行。

snapshot 与事件。 原协议存在 revision／cursor 时按其语义处理；不存在时复现旧客户端顺序并核验恢复机制。不能自创 since 字段解决竞态。若无法证明一致性，保持 Synchronizing／只读，不能把潜在缺失数据标成 Live。

有界队列。 原型内部可先设每终端 4 MiB、总计 64 MiB 的待解析预算，用作压测起点；这不是既有协议上限。只允许合并绘制通知，不允许丢弃未解析字节、业务事件或增量变更。慢消费者应背压或按已验证的断开恢复策略处理；不得因此卡死共享 daemon 的其他会话。

3.7 终端解析、宿主行为与输入
PTY reader
  → 有序字节队列
  → 单一终端解析 actor
  → 网格／模式／scrollback／搜索状态
  → 快照与脏区
  → GPUI 主线程绘制

键盘／IME commit／粘贴
  → 焦点与权限检查
  → 终端输入编码
  → 唯一 writer
  → 对应 PTY

OSC 等宿主请求
  → 独立 HostActionPolicy
  → 原生剪贴板／链接／标题等服务

解析器不能直接操作剪贴板、执行命令或访问任意路径。历史重放、重绘和搜索不能重复触发 OSC 副作用。

必须分别实现并验收 Ctrl-C 输入、显式发送 SIGINT、Ctrl-D 输入、关闭输入端、终止进程，不能混为“停止”。IME marked text 只存在于输入组合状态，确认 commit 后才写入终端，且只写一次。GPUI 公开接口明确区分 marked text、replace、commit 对应的处理方法，但接口存在不构成项目运行通过的证据。
GitHub

编辑框可以提供原生撤销；终端不能把已经提交给 shell 的命令伪装成本地可撤销文本，更不能把 Cmd-Z 自动变成进程控制操作。

3.8 存储与迁移事务

先区分四类数据：

类别	处理原则
既有业务 schema	原格式、字段、ID、时间、排序和错误保持兼容
mounted intent 等持久意图	原样保留，不能被启动过程中的临时未挂载状态覆盖
运行时观测状态	连接、PID、实际挂载、暂时错误等不得未经原规则写回持久意图
新 GUI 独有偏好	独立 sidecar 保存，不借重写旧业务文件加入未知字段

Herdr daemon 自有数据、远端路径、socket、PID 文件和运行目录不进入 GUI 迁移白名单，除非源码明确证明其中某项属于原客户端迁移范围。

迁移顺序固定为：

Preflight
 → FreezeLocalWriters
 → Backup
 → CopyToDestinationStaging
 → Verify
 → AtomicSwitch
 → ReadBackAndHealthCheck
 → Commit
 → ResumeWriters

具体执行规定：

步骤	必须执行的动作	失败处理
预检	校验源／目标身份、白名单、容量、权限、卷、符号链接和嵌套路径；拒绝空路径及未经识别的空目录	不改变原目录，不把空目录当新建成功
冻结	停止 GUI、MCP、同步器、恢复任务、模块宿主等本地写入者，保持原停止顺序	无法证明全部停止则中止；不靠杀共享 daemon 解决
备份	生成一致快照、清单和校验值；存在数据库时包含其一致性要求	备份不可读／不完整则禁止继续
staging	在目标卷创建独立 staging，只复制白名单条目，保留必要权限与元数据	只清理属于本次事务的 staging
验证	数据解析、字段与关联、数量、hash、关键业务读回、Keychain 可访问性	不切换，保留诊断和旧数据
原子切换	使用既有兼容的数据根入口切换，或经验证的平台原子目录交换	不把“两次 rename”当作天然原子；没有可用方案则阻塞
健康检查	新根只读加载、组件检查、旧 schema 对照，尚不开放业务写入	在未产生新写入时切回原根
提交	持久化事务状态后恢复写入与原 mounted intent	不重新执行已经完成的外部副作用
提交后回退	先冻结并保存新数据，再判断旧版本是否能够读取或安全反向迁移	禁止直接用旧备份覆盖已经产生的新写入

迁移 journal 放在独立事务区域，不更改旧 schema。每个状态必须可在崩溃重启后判定“继续、回退或要求人工处理”。

一个重要限制：新程序加文件锁，不代表旧程序会遵守这个锁。 如果旧 GUI／旧 MCP／旧同步进程不配合锁协议，必须确认其已经停止，才能切换数据根。

Keychain 不属于普通目录复制。旧／新签名身份、访问组、service/account、ACL 等都可能影响读取，应分别审计；不能因为新应用能创建一个同名项，就认为它已经读取了旧秘密。Apple 的 Keychain 与签名文档明确将访问控制与访问组或代码签名要求关联。
Apple Developer
+1

3.9 MCP 与原生菜单

MCP、原生菜单、快捷键和 GUI 都调用同一业务入口，不各自实现 CRUD／stop／uninstall。菜单是否启用、目标对象是谁、是否需确认，都来自当前命令上下文。

MCP 必须保持既有协议版本、传输方式、工具名称、schema、返回结构与错误行为，不能“顺便升级到最新版”。例如公开 MCP 文档中，2026-07-28 修订已经改变 Streamable HTTP 的部分生命周期行为，这正说明最新版不等于本项目兼容基线。
Model Context Protocol

MCP 只连接唯一业务写入者。若既有 MCP 支持 GUI 关闭后继续工作，同一 Rust 业务服务应提供相应无窗口运行方式；驻留规则必须按旧行为核验，且该服务不等于 Herdr daemon。 未确认前不得额外建立后台常驻进程。

4. 全能力矩阵

以下“现状”均指摘要基线，不是实测结论。验证必须与旧实现对照。

为避免重复，回滚代码定义如下：

代码	回滚含义
RH	保留 daemon／业务 session，停止新客户端变更，使用原连接机制或旧客户端恢复；不重建业务会话
RV	恢复最后有效 GUI 状态／布局，不改业务对象
RD	冻结本地写入，执行一致性事务回退；已有新写入时先保存新数据
RP	仅清理经过确认属于 GUI 的 PTY／子进程
RI	恢复上个有效组件包、安装记录及挂载意图
RK	保留原秘密与引用，停止相关操作，提示权限／解锁问题
RE	外部副作用不可假定可撤销；查询实际结果、展示未知／部分完成，不自动重放
4.1 S1：Goose Herdr
能力	现状	目标承载	兼容性要求	验证方式	失败回滚
设备／workspace／space／agent session sidebar	摘要要求保留	Herdr 状态投影 + GPUI 树	标识、层级、显示状态与选中关系不变	相同 snapshot／事件下新旧树对比	RH、RV
分组、拖拽、排序与状态	摘要要求保留	分组服务 + 树交互	原排序规则、持久位置、失败行为；明确原数据 owner	拖动、重启、冲突和失败回退	RD 或 RH
snapshot／catalog	摘要要求保留	协议层 + 一致性恢复	字段、空值、未知字段、事件时序保真	黄金样本、分片、乱序与恢复差分	RH
pane 操作	摘要要求保留	命令路由 + 原 Herdr 方法	GUI pane 与服务端 pane 含义不混淆	新建／关闭／操作失败及真实服务端检查	RH、RE
newline JSON-RPC	摘要明确	原始 framing + DTO	不改方法、ID、字段、错误结构	每字节分片、合包、非法帧和 EOF	RH
订阅与重连	摘要要求保留	连接状态机	原订阅顺序、退避、恢复语义	断线、旧事件、恢复期间新事件	RH
agent／terminal attach	摘要要求保留	attach controller + 唯一 broker	一个 attach 实例一个 owner，不重建业务 PTY	真实 CLI、输入输出、两客户端检查	RH、RP
takeover	摘要要求保留	原服务端机制	不伪造客户端租约；拒绝／冲突行为不变	两客户端竞争及超时结果核对	RH、RE
ended overlay／显式 reconnect	摘要要求保留	session 状态投影	ended 与 disconnected 分离；reconnect 不变新建	服务端结束、网络断开、客户端退出三组测试	RH、RV
递归 split tree／比例	摘要要求保留	递归布局模型	结构、比例、叶子标识、恢复规则不变	深层树、极端比例、重启与窄窗	RV
四向 focus／swap	摘要要求保留	焦点与布局命令	原方向选择和 swap 语义；不交换 owner	不规则分栏下键盘路径与 session 标识核对	RV
Tailcat WireGuard	摘要要求保留	原传输适配	既有 token、版本、连接拓扑	实际直连／受限网络／失联恢复	RH、RK
SSH fallback／stream-local forward	摘要要求保留	SSH 适配	原 fallback 条件；主机身份和 socket 语义不降级	路径不可达、主机键异常、转发结束	RH
local socket／IPC	摘要要求保留	本地传输层	原路径、权限、peer 身份、超时和错误	无权限、陈旧 socket、错误类型对象、并发	RH
Keychain token	摘要要求保留	native Keychain 服务	原属性及签名访问行为，不转明文	锁定／拒绝／旧项／重新签名	RK
附件上传	摘要要求保留	上传服务 + 原协议	原字段、大小、取消／重试／错误语义	大文件、取消、重复确认、断线和失败	RE
命令生成	摘要要求保留	原命令规则适配	argv／shell quoting、cwd 和 env 不变	空格、引号、Unicode、特殊参数对照	RE
字体／字号／行距／鼠标偏好	摘要要求保留	偏好服务 + renderer	原值、默认值和持久格式保留	重启、缩放、选区和鼠标模式	RV、RD
原生菜单	摘要要求保留	native menu + 命令路由	焦点、启用状态、快捷键、错误一致	真实菜单点击及不同焦点上下文	RV
HerdrKit 客户端重写	摘要明确	Rust 协议适配	daemon／CLI／socket 契约与 binary 策略不变	旧服务端配新旧客户端完整对照	RH
4.2 S2：Hub 全业务与组件，必须独立保留
能力	现状	目标承载	兼容性要求	验证方式	失败回滚
Hub A/B/C 布局与主题	条件范围	统一壳与偏好	同能力，不通过布局删减功能	同一任务集跑三布局、主题重启	RV
商店浏览／详情懒加载	条件范围	catalog 服务	原字段、缓存、加载与错误行为	慢网、离线、缓存失效、重复进入	RV
下载／校验／重试	条件范围	下载与安装事务	原完整性依据和重试语义；校验成功后才能安装	截断、hash 不符、损坏包、路径穿越	RI
已安装／启停／卸载	条件范围	组件管理器	不混淆禁用、停止、卸载和删除用户数据	每种状态转换及中途崩溃	RI、RD
挂载／卸载	条件范围	模块生命周期服务	保留原 hooks、依赖、停止顺序	部分挂载失败、卸载失败、重入	RI
mounted intent／启动恢复	条件范围	持久意图 + 恢复协调	不将临时失败写成用户取消挂载；避免重复执行	启动各边界崩溃、依赖缺失、恢复重入	RI、RD
快捷键录入／冲突／注册注销	条件范围	native shortcut 服务	原组合键表示、冲突与占用规则	录入取消、重复键、系统占用、退出清理	RV、RD
app-only／launcher	条件范围	应用运行模式	原启动、隐藏、激活、退出与恢复语义	Finder、快捷键、重复启动、关闭窗口	RV
命令面板／原生菜单／窗口生命周期	条件范围	统一命令系统	与组件可用性、焦点及权限一致	多窗口、窗口关闭、隐藏应用、Cmd-Q	RV、RP
数据目录迁移／备份／回滚	条件范围	迁移协调器	原白名单、空目录拒绝、停止顺序和原子切换	全事务故障注入与旧版读回	RD
旧数据导入	条件范围	格式兼容服务	原字段、ID、分组、排序、重复与错误规则	所有旧格式、损坏文件、部分失败	RD
Hub MCP 适配	条件范围	MCP bridge	原工具、传输、schema、错误与生命周期	真实客户端和原客户端差分	RD、RE
2FA：OTP、分组	条件范围	OTP 服务 + GPUI	原算法参数、计数／时间、分组、顺序与秘密引用	固定时刻／计数黄金向量、新旧值对照	RD、RK
2FA：导入、二维码	条件范围	导入与二维码服务	支持的原格式、重复处理、参数保真	全格式样本、损坏码、多账户、取消	RD
2FA：回收站、安全存储、MCP	条件范围	业务服务 + Keychain + MCP	恢复原 ID／关系；秘密授权不扩大	删除恢复、锁定、权限拒绝、工具调用	RD、RK
Marks：CRUD、分组、排序、搜索	条件范围	Marks 服务	原字段、搜索匹配、排序、重复规则	操作序列及重启差分	RD
Marks：模板 URL、导入导出	条件范围	模板与格式兼容层	替换、转义、编码和原格式不变	特殊字符、空参数、多语言、往返导入	RD、RE
Marks：缓存、死链检测	条件范围	缓存／检查任务	原有效期、状态分类和网络权限边界	超时、重定向、权限与网络异常	RD、RE
Marks：同步、MCP	条件范围	原同步契约 + MCP	不把单向变双向；保留冲突、删除和错误语义	双端变更、断线、重复事件、删除恢复	RD、RE
Marks：uTools-only	摘要明确限制	宿主能力适配	无真实替代或桥接时明确宿主受限，不伪造成功	原宿主与目标宿主实际对照	不触发副作用；列入差异门禁
Monitor：进程、窗口、端口、网络	条件范围	macOS 监测服务	原字段、采样、权限拒绝和不可见状态	系统数据对照、权限变化、进程退出	RV、RE
Monitor：PID 二次校验、分组、偏好、MCP	条件范围	控制服务 + 统一命令	操作前重新核验身份；不只凭旧 PID 执行	PID 重用／进程替换、拒绝权限、工具调用	RE、RD
Run：脚本 CRUD、分组、搜索	条件范围	Run 服务	原脚本内容、字段、ID、排序不变	旧数据加载、编辑、重启、搜索对照	RD
Run：run／stop、cwd／env／shell／确认	条件范围	Run controller + broker	原解释器、参数、工作目录、确认及退出行为	各原脚本类型、错误 cwd、特殊 env、停止	RP、RE
Run：脚本与窗口旧数据	条件范围	存储兼容层	不因 Rust 化重解释旧脚本；失效 PID 不当存活	全量往返、新旧版重开、崩溃恢复	RD
Run：session/free、最多 8、旧 native addon 边界	条件范围	Run 兼容 API + 唯一 broker	配额范围、计数时机、释放、句柄和错误需逐项保真	1–8、第 9 个、失败创建、重复 free、残留句柄	RP、RD

**uTools-only 的验收处理：**必须拿到实际能力清单，确认哪些本就只在 uTools 宿主成立。目标环境没有对等能力时，需要真实桥接或明确批准的兼容差异；一个禁用按钮不能被计为“已迁移完成”。

4.3 共用终端能力
能力	现状	目标承载	兼容性要求	验证方式	失败回滚
shell／env／cwd	摘要要求	对应 owner 的启动适配	原登录 shell、环境合并与路径语义；远端 cwd 不按本机校验	原命令、Finder 启动环境、远端路径	RP、RH
session/free／最多 8	Run 专属要求	Run API	不成为 Herdr 全局限制	配额、失败占位、结束未 free、重复 free	RP
resize／输入校验	摘要要求	broker + 解析 actor	字节顺序、尺寸、隐藏窗与最小尺寸规则	高频缩放、零尺寸、非法参数、切屏	RP、RV
SIGINT／EOF	摘要要求	输入／控制分离	Ctrl-C、SIGINT、Ctrl-D、关闭流、终止分开	shell、子进程、raw mode 和 job control	RP、RH
VT／ANSI、颜色与字符宽度	摘要要求	terminal core	原支持序列；CJK、组合字符与实际字体一致	固定语料、随机分片、网格差分	RV；阻塞不兼容路线
scrollback	摘要要求	有界历史模型	原配置、滚动锚点和文本语义	大输出、清屏、重排、后台继续输出	RV
选择／复制／粘贴	摘要要求	输入与原生剪贴板服务	空白、换行、Unicode、矩形／词选择按基线	复制文本逐字节对照、多行粘贴	RV、RE
搜索	摘要要求	历史搜索 + UI	原范围、匹配与跳转行为	Unicode、跨行、大历史、取消	RV
alternate screen	摘要要求	解析模式与双屏状态	进入／退出、恢复、滚动和选择不串屏	编辑器、分页器等真实 TUI	RV
鼠标／bracketed paste	摘要要求	模式感知输入编码	应用鼠标模式与本地选择分开，粘贴包裹按模式	不同鼠标协议、模式切换、长粘贴	RV、RE
OSC／bell／光标	摘要要求	解析状态 + 宿主策略	逐项列出既有 OSC 和权限；不重放副作用	标题、链接、剪贴板、查询、光标样式	RV、RE
重启／退出码	摘要要求	owner controller	本地 attach 退出码不冒充远端业务退出码	正常／异常退出、signal、重启竞态	RP、RH
IME marked text／commit	摘要要求	GPUI 输入 adapter	组合态不写 PTY，commit 不重发，候选框位置正确	中文／日文／韩文、切焦点和多屏	RV；未通过阻塞发布
剪贴板与撤销	摘要要求	原生命令 + 编辑上下文	本地编辑撤销与终端输入分离	菜单、快捷键、IME、已提交文本	RV
窄窗／高 DPI	摘要要求	布局与字体度量	不通过缩窗删除 session 或改变 split tree	极窄窗、不同屏幕、字号放大	RV
后台读取解析／脏区绘制	摘要要求	reader → actor → snapshot	仅丢弃冗余绘制通知，不丢原始数据	高吞吐、后台页、慢消费者、泄漏压测	RP、RH
4.4 全局安全与可访问性
能力	目标承载	验证	失败默认
秘密不进日志	结构化日志白名单；禁记 token、OTP seed、原始终端流和附件正文	合成秘密注入，扫描日志、错误、崩溃材料与 MCP 日志	停止日志／相关操作，不假装脱敏成功
路径与权限	统一文件服务、授权根、身份校验、最小权限	符号链接、目录替换、越界路径、权限拒绝	拒绝操作，保留原数据
键盘焦点	统一焦点模型	全键盘完成核心任务，弹窗结束恢复焦点	阻塞相应验收
label／role／name／state	稳定 ID 的可访问性树	Inspector 与实际 VoiceOver	不宣称支持
reduced motion	系统偏好映射	禁用非必要动画、光标和提示替代	保持静态可用
自绘终端 VoiceOver	终端文本语义、选择、状态与动作桥接	真机读文本、操作和高输出场景	列为未通过，不以截图或接口存在代替
5. 风险登记表
风险	级别	主要触发点	可执行缓解措施	停止／回退条件
双 spawn／双 owner	致命	renderer 与 broker 同时创建 PTY；Drop 越权清理	三类 owner 类型化；统一 broker；禁止共享会话持有本地 kill handle；所有生命周期路径测试	无法证明单 owner 就禁止连接真实 session
协议／错误不兼容	致命	DTO 丢字段、精度变化、错误统一包装、擅加重试	原始层保留；黄金样本；新旧差分；未知结果状态	任一既有契约样本不符，停止相关写操作
订阅恢复丢状态	高	snapshot 与事件竞态、旧连接回调覆盖	复现原同步顺序；generation 隔离；有证据的 resync	无一致性证据时不显示 Live、不开放依赖状态的变更
许可证不合规	致命	GPL 源码抽取、资产漏标、fork 来源不清	文件级来源表、依赖闭包、NOTICE、分发审查、补丁来源记录	授权与分发义务不清则禁止分发对应路线
数据损坏／回滚覆盖新数据	致命	多写入者、空目录替换、备份不一致、提交后盲回滚	冻结所有本地写入者；白名单；journal；切换后先只读；保留新写入	任一校验不符立即停写，不能继续迁移
Keychain／签名变化	高	新应用读不到旧 token／seed	原属性审计；旧新签名真机验证；不自动删建同名项	旧秘密未可读前禁止切换
IME／VoiceOver 回归	高	自绘文本、错误 UTF-16 范围、重复 commit、语义树不稳定	输入 adapter 专项测试；稳定语义 ID；真机辅助技术测试	未通过即不宣称支持，不能只发“稍后补齐”版本
性能／内存与慢消费者	高	每字节通知、全屏重绘、无界队列、隐藏页不读取	背景解析、有界预算、脏区、虚拟化、按帧合并通知	数据丢失、持续增长或共享 daemon 被拖死即失败
升级／降级	高	旧版忽略不了新 schema、Keychain 访问改变	业务 schema 不变；新偏好 sidecar；双向重开；保留应用和数据回退点	无损回退不可证明时先保存新数据，不自动降级
MCP 越权／副作用重复	致命	另建写入器、无限权限、取消后自动重试	同一命令入口；工具级授权；结果未知状态；禁止记录秘密	无法辨认调用授权或执行结果则拒绝／停止重试
Monitor 误杀进程	致命	只凭列表缓存 PID 操作	操作前二次核验 PID、启动身份、用户与目标；尽量使用身份约束接口	身份变化立即拒绝；不能声称消除了平台上所有残余竞态
包／路径攻击	高	未校验下载、解压越界、加载任意 native addon	验证原包信任机制；路径和解压预算；只加载批准格式	校验失败不安装，不以重试绕过

如果既有行为与强制安全门禁冲突，应登记明确的兼容性例外并获得批准。不能一边悄悄禁用功能，一边声称“所有行为不变”。

6. 三个交互原型与评审标准

以下是可供实现的文本交互原型规格，不是已经完成的 GPUI 运行原型或私有截图复刻。

A：高保真工作台——默认
顶部：当前 workspace／全局搜索／命令入口／连接状态
左侧：设备 → workspace → space → session，支持分组、状态、拖拽
中央：当前能力页面或递归终端 split tree
右侧：按需展开的详情、附件、属性或操作结果
底部：焦点目标、连接路径、运行状态、错误／重连入口

默认保持层级导航可见，详情栏按需打开。S2 下，商店和四组件进入同一导航体系，不塞进终端右键菜单里。

B：终端专注
顶部：紧凑 workspace／session 切换与完整命令入口
中央：最大化终端区域
侧栏／组件／详情：抽屉或可固定面板
底部：必须保留连接、控制权与结束／失联状态

所有管理能力仍有完整页面和键盘入口。进入 B 不能关闭后台组件，也不能取消事件订阅。

C：多任务分栏
左侧：共享对象树
中央：递归分栏，允许终端与非终端能力面板并列
焦点区域：高亮当前命令目标和 session 身份
详情区：跟随焦点，但不自动改变业务选择或控制权

不能把 C 实现成固定的四宫格来替代递归 split tree；也不能为了“同时显示”给同一个 session 多创建一次 attach。

三原型统一评审
维度	权重	判定方法
能力与状态语义完整	35%	三布局完成相同任务；session 数、attach 数、mounted intent 不随切换变化
键盘与可访问性	25%	完整键盘路径、焦点恢复、VoiceOver、IME、字号放大
操作效率与状态清晰	20%	找到目标、识别当前控制对象、恢复断线、管理组件所需步骤
布局与性能	10%	窄窗、多屏、高输出、多分栏与后台面板
视觉质量	10%	层级、间距、文字可读性、选中／危险／失联状态区分

**一票否决项：**能力被删减、视图切换改变 owner、数据丢失、焦点错误导致输入到另一 session、未确认时执行危险动作。

必须给每种原型提供同样的正常、加载、空、权限拒绝、断线、冲突、ended、恢复失败、迁移只读状态。没有用户测试数据前，不伪造三者评分；A 是架构默认，不是已经被证明最优。

7. 迁移门禁与分层验收
7.1 迁移前硬门禁

迁移前必须具备：范围签收、旧实现可运行基线、协议样本、三路 owner 证据、存储 schema／白名单、真实停止顺序、Keychain 访问验证、版本与许可证锁定，以及至少一次完整故障恢复演练。

不满足其中任何影响安全的条件，就不进入真实数据写入。

7.2 分层验收清单
层级	必须执行	通过标准	不能替代它的证据
L0：源码与契约	构建入口、依赖、协议、IPC、owner、错误、权限、许可证审计	清单可追溯到源码／二进制／样本，无关键空项	摘要、推测、公开同名项目
L1：编译与依赖	cargo metadata/tree、格式、clippy、测试、release build；使用锁定依赖和合法 feature 组合	支持的每个 macOS／架构目标可重复构建，依赖来源一致	本机 debug 编译成功
L2：协议与业务	黄金样本、新旧差分、分片／乱序／故障测试、存储往返	原字段和错误一致；副作用计数一致	UI 显示正确、mock 返回成功
L3：真实 runtime	从 .app／Finder 启动；真实 PTY、菜单、窗口、输入、退出和崩溃	行为符合原基线，资源无越权清理	cargo run、静态截图
L4：视觉	A/B/C、主题、窄窗、字体、缩放、多屏截图与交互检查	无遮挡、裁切、焦点错位、状态误导	一张理想尺寸截图
L5：CJK IME	中文、日文、韩文；候选、组合、取消、commit、切焦点、多屏、长文本	marked text 不泄入 PTY，commit 恰好一次，候选位置正确	能输入 ASCII、存在输入接口
L6：VoiceOver	启动前／后开启；树、菜单、弹窗、终端文本、选择、状态变化、高输出	核心任务可完成，语义准确，不丢焦点、不反复整屏播报	Inspector 有节点、框架声明支持
L7：MCP	使用既有真实客户端连接；工具枚举、调用、错误、授权、取消、断线、GUI 退出	既有契约与生命周期不变，日志无秘密	只跑工具函数单元测试
L8：真实 Herdr	对可丢弃测试环境运行旧 daemon／CLI；本地与远程、两客户端、各传输路径	attach／takeover／ended／reconnect、字段、错误和 owner 全部一致	fake daemon 或录制响应
L9：迁移与升级	每个 journal 边界故障注入；磁盘满、无权限、并发、重启；新旧版重开	数据、mounted intent、秘密引用与回退均可验证	文件数一致、复制命令退出 0
All：发布	签名、权限、许可证产物和最终范围验收	选定范围全部通过，差异均明确批准	“主要功能能用”

编译矩阵不能盲目运行 --all-features 来替代产品组合测试；TA／TB 或不同宿主模式可能应互斥。Apple Silicon 是必要候选测试目标，是否同时要求 Intel、最低 macOS 和其他版本，必须从现有支持范围确认，不能擅自缩小。

7.3 真实 Herdr 的关键必测场景

必须在隔离测试环境验证下列不变量：

场景	必须观察到的结果
GUI 正常退出、强制退出、崩溃	共享 daemon 与业务 session 未被 GUI 越权终止
attach client 退出	不自动等价于 server session 关闭
standalone shell 结束	仅影响对应 standalone 对象
两客户端 takeover 竞争	与旧客户端的成功／冲突／控制权结果相同
控制通道断开、PTY 通道仍有数据	UI 不错误显示 ended，也不把输入送入错误目标
断线期间服务端 session 已结束	重连后根据权威状态显示 ended，不新建替代 session
高输出与慢客户端	其他共享会话不被新客户端的队列策略拖死
权限拒绝、token 失效、SSH 身份异常	原错误语义保留，不静默切换到更弱信任路径

若摘要中的 Tailcat 就是公开的 tailscale/tailcat，其官方说明明确不承诺 API、CLI 和 wire format 稳定性；因此既有版本核对与实际链路测试是硬门禁，不能用公开“最新版本”替换来证明兼容。
GitHub

7.4 性能预算

建议先采用以下原型目标，再用旧实现和指定硬件校准，不能把它们写成已经达到的性能：

项目	原型目标
本地按键到呈现	常规负载下 p95 不高于 50 ms
60 Hz 前台交互	主线程帧处理 p95 尽量控制在约 16.7 ms 内
大输出	不丢解析字节、不无限增长、UI 仍可切换与取消
后台终端	继续读取解析；降低绘制频率，不停止状态消费
资源稳定性	反复创建／attach／关闭至少 100 次，无持续增长的 FD、线程、子进程和内存
多会话	Run 验证其 8 个配额；Herdr 按实际支持规模另测，不共享这个上限

GPUI 已有 AccessKit 相关接口与文档，并强调稳定节点身份和可访问性动作处理；这只是可实现的基础，不是自绘终端自动获得完整可访问性的证明。
GitHub

8. 全部未决项、所需证据与安全默认

阻塞级别定义：

**B0：**证据缺失时禁止相关真实数据／进程副作用，或禁止路线进入正式实现。

**B1：**允许隔离原型，但禁止选定范围发布。

**B2：**可采用明确的无副作用默认，不阻塞基础设计。

下表的“默认”只用于未核验阶段，不能把功能暂时禁用当作最终完成。

未决项	必须取得的证据与核验方法	级别	缺失时安全默认
S1 还是 S2 获批	范围 ADR，逐项确认矩阵	B0	以 S1 设计为推荐，S2 保留；不裁剪、不发布
Herdr 构建与二进制来源	入口、manifest、lock、构建脚本、版本、hash	B0	不替换、不重编 daemon，不声称客户端兼容
daemon／CLI 自动启动行为	启动代码、进程树、PID／socket 产生过程	B0	不新增后台启动或退出清理逻辑
三路 PTY 与进程所有权	spawn／wait／kill／Drop 路径及实际进程观察	B0	不连接真实 session 执行关闭／停止
attach EOF／signal 是否影响服务端	原 CLI 源码及真实 detach／kill／崩溃测试	B0	不假定“杀 attach 就安全 detach”
attach／takeover／ended／reconnect 语义	原操作序列、错误、竞争与恢复样本	B0	不自动 takeover、不自动新建替代 session
framing、字段、错误、超时	全方法目录、raw 样本、非法输入和超时结果	B0	只做合成数据原型，不开放真实写请求
订阅与 snapshot 一致性机制	revision／cursor 或原同步顺序及竞态测试	B0	不宣称无缝恢复；状态未确认时只读
tree／pane／split 的数据 owner	原 schema 与读写调用路径	B0	不把 GUI 布局直接写回服务端对象
Tailcat 是否对应公开项目及实际版本	当前依赖、binary hash、地址格式、连接代码	B0	原样保留，禁止自动升级或重解释 token
SSH／local socket 信任策略	路径、权限、peer 校验、主机键和 fallback 条件	B0	不接受未知身份，不降级绕过认证错误
GPUI／Rust／终端／PTY 最终 pins	同一基线构建、runtime 和许可证结果	B0	候选只用于隔离验证，禁止浮动依赖发布
TB 许可证与分发模式	文件级许可证、依赖／资产清单、分发审查结果	B0	不将 GPL 代码混入 TA，不分发 TB
schema、迁移白名单、空目录语义	原存储读取／写入／迁移源码与旧数据样本	B0	拒绝自动迁移；首次新建走独立流程
mounted intent 与停止顺序	持久字段、模块 hooks、依赖及退出代码	B0	不把临时 observed 状态覆盖 intent
Keychain 类型、属性与签名身份	实际旧项属性、签名／entitlements、解锁与拒绝测试	B0	不删除、覆盖或导出明文秘密
切换与提交后回退机制	文件系统和配置入口、故障注入、旧版读写报告	B0	不切换；已有新数据时不盲恢复旧备份
商店包格式与校验信任源	manifest、签名／hash、下载／重试／安装样本	B0／S2	不执行未验证包，不用“下载成功”代替安装成功
组件宿主与 native addon 契约	生命周期 hooks、ABI／IPC、参数、错误和释放规则	B0／S2	不假定 Rust UI 能直接运行原组件
2FA 算法／格式／回收站／MCP 秘密权限	旧 fixture、固定向量、全部导入格式和授权规则	B0／S2	不改写秘密或批量导入生产数据
Marks 同步、模板与 uTools-only 清单	同步方向、冲突／删除规则、模板语法、宿主 API	B0／S2	不自创双向同步，不伪造宿主能力
Monitor 数据来源与 PID 操作	平台 API、权限需求、进程身份和失败测试	B0／S2	无法确认身份则不执行进程控制
Run 解释器、旧脚本与窗口数据	原启动参数、环境规则、脚本类型、窗口 schema	B0／S2	不把 Node／其他脚本自动改用 Bun，不自动执行恢复脚本
Run 的“8”具体计数口径	原 session/free 实现与边界错误样本	B0／S2	不推广到 Herdr；不猜 active／allocated 计数
MCP 版本、工具、传输、无窗口行为	实际客户端配置、schema、返回、授权与退出记录	B1	不自动升级协议，不额外暴露端口或后台服务
附件和命令生成细节	大小／类型、取消、上传参数、shell quoting 基线	B1	不自动重试未知结果的上传或命令
终端 VT／OSC／键位／TERM 基线	原终端配置与完整语料、实际 TUI	B1	不虚报终端能力；未支持序列列为差异
CJK IME 与 VoiceOver	目标 .app 真机测试、屏幕／辅助技术证据	B1	标记未验证，不能宣布已支持
macOS、架构、分发与权限范围	原支持声明、签名和 sandbox／entitlement 需求	B1	不缩小既有支持范围，不承诺未测试平台
数据规模与性能预算	旧实现基准、真实规模的脱敏分布、压力测试	B1	使用有界原型预算，不宣称性能达标
私有视觉基线与默认偏好	获授权的参考和实际用户评审	B2	使用原创 A 布局规格，不声称忠实复刻

对于无法通过仅客户端改造保持的既有行为，处置只有三种：补足证据、实现真实兼容层、或明确批准兼容性例外。 “先删掉以后再做”不属于本计划的完成路径。

9. 最容易造成回归或损坏的前三项：建议直接替换进方案的修订稿
修订一：用显式所有权契约替换“统一终端 session 管理”

终端显示对象不是业务 session owner。
系统分别建模共享 Herdr 业务 session、attach client PTY 与 standalone shell PTY。GUI 的 PTY broker 是其所拥有 PTY 的唯一创建和释放入口。任何 renderer 构造、析构、布局切换、窗口重建均不得隐式创建或终止 Herdr 业务 session。
Hub Run 的 session/free、配额 8 和既有 native addon 释放语义只作用于原 Run 范围。GUI 退出不得终止共享 Herdr daemon；attach client 退出不得被直接解释为 server session 结束。

**必须附带的验收：**GUI 崩溃、窗口关闭、100 次 attach／detach、两客户端 takeover 后，核对服务端 session 和业务进程身份；检查 GUI 仅清理自身资源。

**默认失败处理：**暂停相关进程副作用，保留已有服务端 session，不自动重建。

修订二：用“协议保真 + 结果未知”替换“统一错误／自动恢复”

客户端迁移不得改变 Herdr 的 wire 契约、业务字段、错误结构与原有恢复语义。
请求／响应／事件建立原始保留层与类型化访问层，未经批准不得丢弃字段、改变数字精度、缺失值、null 或 ID 表示。连接状态、attach 状态与业务 session 状态分别建模。
非幂等操作在发送后超时或断线，进入“结果未知”，不得自动重试或报成功。恢复只使用既有协议可验证的机制；没有一致性证据时不开放依赖该状态的写操作。

**必须附带的验收：**旧客户端与新客户端跑相同请求、异常输入和故障序列；对比字段、错误、事件和实际副作用次数。

**默认失败处理：**保持可观察、可查询的只读状态；不假装取消了远端执行。

修订三：用“有提交边界的迁移事务”替换“复制后切换，失败恢复备份”

数据迁移必须先冻结全部本地写入者，再执行一致备份、白名单 staging、校验、原子切换和只读健康检查，最后提交并恢复写入。
保留原 schema、mounted intent、停止顺序、空目录拒绝与目录白名单；新 GUI 偏好和迁移 journal 不污染旧业务格式。Herdr 共享 daemon 不因 GUI 数据迁移被停止。
切换前和切换后未开放写入时可以回退；一旦提交并产生新数据，必须先保存新数据、确认旧版本读取能力或安全反向迁移方案，不得直接用旧备份覆盖。Keychain 可读性是切换前条件，不是迁移后补救项。

**必须附带的验收：**在每个 journal 状态注入崩溃、磁盘满、权限变化和旧进程并发写入；验证重启后的恢复决策，以及新旧版本对相同业务数据的读取。

**默认失败处理：**停写、保留旧根和新数据证据；不删除未知目录，不报告迁移成功。

最终判断：这个项目应该围绕“兼容契约、所有权、可恢复写入”组织，而不是围绕“把界面换成 GPUI”组织。 S1 + A 是我推荐的默认架构；TA 是优先验证路线；S2 和 TB 都有完整、同轮的实施与验收路径。当前尚未具备把任何路线标为“已批准实施”或“已验证兼容”的证据。

✨ 最值得优先投入的是三份可复用证据：旧客户端黄金行为集、三路 PTY 生命周期测试、迁移故障注入集。它们不仅约束这次迁移，也能直接成为之后每次 GPUI、终端库和客户端升级的回归门禁。

---

## 文内来源链接（DOM 提取）

- [GitHub +2 GitHub +2](https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal/Cargo.toml)
- [Docs.rs](https://docs.rs/alacritty_terminal/latest/alacritty_terminal/)
- [GitHub](https://raw.githubusercontent.com/zed-industries/zed/main/crates/terminal/src/terminal.rs)
- [GNU +1](https://www.gnu.org/licenses/gpl-faq.en.html)
- [Docs.rs](https://docs.rs/portable-pty/latest/portable_pty/)
- [GitHub](https://raw.githubusercontent.com/zed-industries/zed/main/Cargo.toml)
- [GitHub](https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/src/input.rs)
- [Apple Developer +1](https://developer.apple.com/documentation/security/sharing-access-to-keychain-items-among-a-collection-of-apps)
- [Model Context Protocol](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [GitHub](https://github.com/tailscale/tailcat/blob/main/README.md)
- [GitHub](https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/src/_accessibility.rs)
