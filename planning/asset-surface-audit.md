# 图片与缺省态审计（2026-09-19）

仅源码审计；未启动应用或验证视觉。现有 SVG 工具图标、Agent 品牌图标、项目类型图标及 app_mark 保留，不让 AI 重画。建议批量生成 5 张无文字、无品牌、低对比装饰插图：workspace / search / files / usage / connection；同一张可跨主题使用的中性构图优先，文字与操作仍由 GPUI 渲染。

## 界面清单

| 界面 / 位置 | 当前状态 | 建议 | 接入边界 / 风险 |
| --- | --- | --- | --- |
| 工作台 `src/app.rs::render_workspace` | `split == None` 一律显示 48px terminal 图标和新建/独立终端/重连按钮 | workspace 主插图；未连接时 connection 插图 | 必须先区分 loading、连接异常、已连接未选会话；不把未连接解释成尚无空间。保留四个已有按钮及本机条件 |
| 侧栏 `render_sidebar` | `snapshot.workspaces.is_empty()` 文案；无 snapshot 文案 | 保留小型 folder / terminal / warning 图标，不塞大图 | 窄侧栏不重复主插图；缺快照不是零空间 |
| 全局搜索 `src/app_search.rs::render_search` | `count == 0` 按 loading 显示查询中或无匹配；下方设备状态 | search 图只用于已完成且结果为零；查询保留文字/原生图标 | 部分设备失败应提示搜索不完整；保留各设备状态，不用一张无结果图覆盖错误 |
| 设置搜索 `src/app_settings.rs::render_settings` | `matches.is_empty()` 导航和正文重复文案 | 正文复用 search 小插图，导航保持文案 | 同一页面不要显示两张插图；这是同步过滤，可信空态 |
| 本地/远程文件 `src/app.rs::render_files` | entries 空时按 loading 二选一，失败仅弹全局错误 | 每一空目录复用 files 插图 | 先补 scoped 错误/成功读取判断；否则关闭错误后会谎报目录为空；保留路径、隐藏项状态和传输动作 |
| 用量弹层 `src/app_runtime.rs::render_usage` | 非本机提示切本机；providers 空提示启用 Agent | usage 小插图用于上述引导空态 | 320px 弹层避免大图挤走操作；不得绘制模拟图表或零用量 |
| 用量卡片 `render_usage_card` / Cursor 账户 | status/failure/history_error、授权/缓存/未知多分支 | 保留真实品牌图标、状态文案与授权动作 | 不在每张卡里重复插图；查询中不是未启用，失败不是零，过期缓存仍标缓存 |
| 终端 `src/app_feedback.rs::render_terminal_feedback` | terminal ended 时叠加警告、原因、重连/接管 | connection 小插图可用，已有 warning 必须保留 | 不遮蔽接管影响说明；不在正常终端输出区放图 |
| 全局错误对话框 `render_feedback` | self.error + 确认操作 | 保留 warning，无需额外图 | 错误内容与操作优先，不把异常美化成欢迎页 |
| 检查器 `src/app_inspector.rs::render_inspector` | loading / error / message；未读取输出 | 保留文字/小图标 | 未读取不是空输出，刷新失败不可清掉可见输出，不需要独立插图 |
| 新建空间/Agent 表单 `render_new_item_sheet` | 无可用空间时提示先新建 | 保留小 folder 图标或文字 | 密集表单不加装饰图；不为图创造额外流程 |
| 设备编辑/选择、设置普通页、工具栏/菜单 | 既有字段、图标及状态 | 全保留；无需新增图片 | 图标按钮依旧需要可访问名称；不替换真实品牌和项目图标 |

## 可复用状态字段与精确接入顺序

1. **Workspace**：`DeviceState` 已有 `loading`、`connection`、`snapshot`、`status`（`src/app.rs` 约 193 行）。在 `render_workspace` 的 `split=None` 分支：loading → 原生加载文案；非健康状态 → connection + 原 status；确认健康才 workspace。注意 connection/snapshot 在失败后可能仍保留！本项目已有健康判断 `connection.is_some() && !loading && status.starts_with("已连接")`（侧栏状态约 3641 行），复用同一规则比仅检查 connection 更安全。独立终端树存在时不影响其显示。
2. **Search**：`render_search` 约 273 行 `count == 0` 内，先 loading；再检查设备状态是否非健康/快照缺失；最后才能显示 search 无匹配。`statuses` 循环目前仅 `loading || connection.is_none()`，不会覆盖连接对象仍存在但 status 已异常的情况，接入时保留/扩充真实异常提示。
3. **Files**：`App` 已有 `local_loading/file_loading`、entries、generation，无持久的分侧错误/读取完成字段。`load_files_side`（约 978 行）启动时清该侧错误；`Worker::Files`（约 1221 行）先核 generation，成功写 entries 并标记成功，失败保存该侧错误且仍保留全局错误。渲染优先 loading → scoped error → 成功且 entries 空 → files；首次未读取不可当空。可以只加两个 `Option<String>` 错误字段再确认所有进入文件页路径都会启动读取，或用最小成功标记消除首次空态歧义。不要为本次引入通用状态框架。
4. **Usage**：只接 `render_usage` 的 `!local` 与 `providers.is_empty()` 两分支即可。`UsageService` 已有 `fetching`、`failures`、`snapshots`、`cursor_failure`；各卡片已有 snapshot.status、usage_metadata.failure_kind、history_error，全部保留。
5. **Connection**：优先工作台未连接/异常分支；可复用 `render_terminal_feedback` 中 `terminal.is_ended()` 后的卡片。`connection_issue()` 与 `error()` 有真实原因，接管语义与一般退出不同，不能用统一“离线”标题替代。

## 最小集结论

五张合理：workspace（主空台）、search（两处复用）、files（两侧复用）、usage（未启用/切本机）、connection（未连接/结束反馈）。不需要 loading/error/settings/agent/logo 的额外批次；loading、错误和紧凑导航保留现有矢量与文本。装饰图片不承载唯一状态信息，不嵌中文字，不包含真实项目/设备/账号。若批次产出不足，优先 workspace → files → search，usage 与 connection 可继续使用既有原生图标，不能以假图或错误空态凑数。
