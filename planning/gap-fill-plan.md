# GPUI 相对 goose-herdr 缺口补齐计划

2026-09-18 源码对照。源：`/Users/eachann/Work/goose-herdr`；目标：`/Users/eachann/Work/goose-herdr-gpui`。本文件是当前实施入口；`final-plan.md` 是迁移前选型快照。

验收：`cargo build --locked` 成功即通过。不宣称真实 daemon / 远端会话 / Keychain / 通知已运行验证。

## 结论

1. **Herdr 底层在。** GPUI 会话状态读写走 goose-herdr 的 unix socket + 换行 JSON-RPC，没有第二套协议。缺了这块才没有意义；当前没有这种 P0。
2. **设置里的 Agent 没有完全复刻。** 偏好 key、bypass 表、启停、路径覆盖、排序持久化已对齐；检查命令、探测集合、显示名、禁用后快捷键、用量账户区、展开详情没有对齐。
3. **Space / 会话理念半复刻。** Space = herdr workspace、Session = pane/agent 两边一致。GPUI 的 All Spaces 不粘性，Priority 只是可见性过滤，All Spaces 下新建是死胡同。
4. **快捷键 ID 与默认键一致。** 录入方式、落盘形状、Reset All、禁用 kind 过滤不一致。

## 0. Herdr 契约（始终优先）

协议面已接线：`ping`(protocol≥17)、`session.snapshot`、`agent.list`、`workspace.list`、`server.agent_manifests`、`agent.start`/`agent.prompt`/`agent.rename`、`tab.create`/`tab.rename`/`tab.move`、`workspace.create`/`rename`/`close`/`move_block`、`pane.read`/`send_input`/`close`、`events.subscribe`（24 种事件）、attach `herdr {agent|terminal} attach --takeover`、本机 `herdr server`、SSH 转发、Tailcat FFI、named session socket。

不改协议名、不另造会话状态源、不绕过 daemon 写 pane/workspace。GUI 只改选择、过滤、表单和设置。

| 优先级 | 项 | 证据 | 修法 |
|---|---|---|---|
| P1 | attach 找不到精确版本 herdr 直接失败 | `src/transport.rs` `binary_selection` | 无匹配时回落 `hb=herdr`，与 Swift `attachBinarySelection` 一致 |
| P1 | 远端 git 永远本地探测 | `src/app.rs` `request_git_metadata` → `git::probe_local` | SSH 走远端 `git -C`；Tailcat 保持 nil；本机仍本地 |
| P2 | 不自动 spawn `herdr server` | `src/transport.rs` 注释故意不夺 daemon | 保持；只保留显式「启动本机 Herdr 服务」 |
| P2 | named session 未跳过 `default` | `src/herdr.rs` `discover_named_sessions` | 枚举时跳过 `default` |

## 1. Agent 设置（未完全复刻）

已对齐：`dev.eachann.goose-herdr` 域；`agent.bypassDefault` 默认 true；bypass 七种 flag 逐字相同并写入 `agent.start` args；`agent.binaryOverrides` / `agents.disabledKinds` / `agents.kindOrder`；known kinds 11 个；禁用后不查用量；Cursor 只读授权 key。

未对齐：

| 优先级 | 缺口 | Swift | GPUI |
|---|---|---|---|
| P0 | 「检查命令」不重探 catalog | `reloadAgentCatalog` | 按钮绑 `UiAction::Refresh`，只重拉 snapshot |
| P0 | 探测集合过窄 | knownKinds hint 全探，含 `cursor-agent` | `installed_agents` 只探 manifest advertised + `"omp"` |
| P1 | 禁用 kind 仍占快捷键 | `visibleSorted` 不挂 key | `apply_shortcuts` 全量绑定，按下才报错 |
| P1 | 显示名 | Oh My Pi / OpenCode | 原始 `omp` |
| P1 | 设置页无用量账户 / Cursor 授权 | 行展开内 `CursorUsageAccountView` | 只在用量面板 |
| P1 | 无展开详情、行内路径、行内状态、拖拽排序 | chevron + TextField + drop | 平铺行 + 弹窗路径 + 上移/下移 |
| P2 | Usage 空态无「Agent 设置」跳转 | `openAgentSettings` | 无 |
| P2 | 不 bump `agents.disabledKinds.revision` | Swift `@AppStorage` 刷新 | 双开 Swift 窗口不会感知 |

不动：bypass 语义、共享偏好 key、`start_agent_when_ready`。

## 2. Space 与会话

定义保持：Space = herdr workspace；Session = pane/agent；独立 shell 是客户端本地会话，不是 herdr pane。

根因：`attach_with_takeover` 无条件 `switch_scope(device, Some(workspace))`，点会话就会离开 All Spaces。连接成功时 `workspace.is_none()` 还会落到第一个空间。

| 优先级 | 缺口 | 修法 |
|---|---|---|
| P0 | All Spaces 不粘性；重启落到第一空间 | `SelectPane` / 通知跳转不改 `workspace=None`；连接恢复尊重持久化的 None |
| P0 | All Spaces / Priority 下 ⌘T / ⌘⇧T / QuickAgent 死胡同 | NewItem 必须带空间选择；无空间时禁止直接 `bail` |
| P0 | Priority 只是 `hidden()` 过滤，仍按当前空间过滤、仍可拖拽 | 忽略 space 过滤；隐藏空间列表；分组 High priority / Other；禁拖拽 |
| P1 | 独立 shell 不进侧栏、不进 ⌘1-9 | 侧栏加 shell 行，编号与 Swift `switchableSessions` 一致 |
| P1 | 新建空间后未复用 root shell | `workspace.create` 返回的 `root_pane` 给随后的 terminal/agent |
| P2 | retained 行在快照外追加、无置灰 | 按 `sortIndex` 原地合并 |
| P2 | 无按住 ⌘/⌃ 编号浮层 | 可后置 |
| P2 | All Spaces 跨全部设备 | 保持当前设备；与 Swift 不同，单独决策后再做 |

新建入口对齐：

- ⌘N = 新建空间（表单即可，目录浏览器非必须）。
- ⌘T = 已选空间则在该空间 `tab.create`；All Spaces / Priority 先选空间。
- ⌘⇧T = 当前空间新终端；All Spaces / Priority 先选空间；选中独立 shell 时另开本地 shell。
- Agent 快捷键 = 已选空间直接 `tab.create` + `agent.start`；否则带预填 kind 的选空间表单。

## 3. 快捷键

22 个通用 ID 与默认 chord 一致；⌘1-9 / ⌃1-9,0 两边都不可重映射；⌃1 = All Spaces。

| 优先级 | 缺口 | 修法 |
|---|---|---|
| P0 | 通用项写成字符串，Swift `JSONDecoder` 整份回落默认 | `save_shortcut_field` 经 `compatible()` 写成 `{key, modifierRaw}` |
| P1 | Reset All 不清 Agent 快捷键 | 同时清 `app.keyboardShortcuts.agentKinds` |
| P1 | 禁用/未检测 kind 仍绑定 | 只绑定 `detected ∩ enabled` |
| P1 | 通用↔Agent 冲突保存成功、绑定期静默丢弃 | 保存期双向检测，含 pi 默认 `alt-p` |
| P2 | 不 bump `app.keyboardShortcuts.revision` | 写入时 +1 |
| P2 | 录入仍是文本框 | 先保证落盘兼容；按键录制可后置 |

close 级联大体对齐（overlay → split → files/settings → pane → space → window）。独立设置窗口、keepWindow 保活列为 P2。

## 4. 其它工作台缺口

协议与 Files/Inspector/Usage/搜索/分屏/附件已对齐。剩余：

| 优先级 | 项 | 位置 |
|---|---|---|
| P0 | 通知 `init()` 不申请权限，全新安装静默无横幅 | `src/notifications.rs`：`NotDetermined` 时 request |
| P1 | SSH 密码失败无交互补录 | 认证失败弹表单写 Keychain 后重连 |
| P1 | 侧栏快捷行开关设置页无 UI，且只启动读一次 | Appearance 增加开关并即时生效 |
| P2 | 终端右键/OSC8、字体下拉、i18n 缺条目、独立设置窗 | 后置 |

## 5. 分阶段实施

每阶段独立可编译。禁止改 RPC 名和 daemon 所有权。共享偏好只写 Swift 已有 key 与形状。

### A. Herdr 加固

- `src/transport.rs` attach CLI 回落。
- `src/git.rs` + `src/app.rs` 按设备类型探 git。
- 可选：named session 跳过 `default`。

### B. Space / 会话

- `SelectPane` 与 All Spaces 粘性：点会话只改 `selected_pane`，不改 `workspace=None`。
- 连接恢复：持久化 None 则保持 All Spaces。
- NewItem / NewTerminal / QuickAgent 增加空间字段；All Spaces 下必选。
- Priority：忽略 space 过滤、隐藏空间列表、分组、禁 `tab.move` / `workspace.move_block`。
- 独立 shell 进侧栏与 ⌘1-9。

### C. Agent 设置

- 新 `UiAction` 真正重跑 `installed_agents` / `Worker::Catalog`。
- catalog 按 `known_agents()` hint 探测（cursor → `cursor-agent`）。
- 显示名表与 Swift `AgentKindDisplay` 对齐。
- 禁用 kind 从 keymap/菜单移除。
- 设置页：行状态文案、路径、检查命令；用量账户可复用现有 Usage 控件，不重做 Swift 展开动画。
- 写 `agents.disabledKinds` 时 bump revision。

### D. 快捷键落盘

- 保存走 `compatible()`。
- Reset All 含 agentKinds。
- 保存期冲突检测。
- bump `app.keyboardShortcuts.revision`。

### E. 通知 / SSH / 快捷行

- 通知启动申请。
- SSH 认证失败表单。
- `sidebar.action.*Hidden` 设置项 + 运行时读取。

样式只在 C/B 的设置行、空间选择表单需要时做；沿用现有 GPUI 设置行/表单，不新建设计系统。

## 6. 风险

1. 共享 `dev.eachann.goose-herdr`。快捷键必须写 `{key, modifierRaw}`；disabledKinds/kindOrder 必须是数组；revision 只允许 +1。
2. `attach()` 改 scope 是 All Spaces 与通知跳转的共同根因，只改这一处，不要在每个调用点打补丁。
3. catalog 扩大探测会让未广告 kind 出现在设置里，这是 Swift 行为；启动仍须 `disabled` + catalog 门禁。
4. Priority 与 Swift 共用 `sidebar.prioritySessions` bool，只改 UI 语义，不改类型。
5. All Spaces 跨设备不做，除非另批；GPUI 仍是当前设备。
6. 不自动抢 `herdr server` 所有权。

## 7. 建议开工顺序

A → B 的 All Spaces/新建 → C 的检查命令与探测 → D 落盘形状 → E 通知。B/C 决定工作台能不能当 herdr 客户端用；拖拽排序、编号浮层、独立设置窗可以后做。
