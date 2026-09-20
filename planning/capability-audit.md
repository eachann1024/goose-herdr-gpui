# Goose Hub 能力与布局扫描

扫描日期：2026-09-18。源码仓库：`/Users/eachann/Work/goose-hub`。本报告只读扫描；工作区已有大量未提交改动，未修改源仓库、未启动服务、未操作 Herdr。

## 1. 宿主架构与真实入口

- Electron 主进程启动、单实例、窗口激活、退出/SIGTERM 清理：`/Users/eachann/Work/goose-hub/apps/host/src/main/index.ts:1-95`；窗口创建与 BrowserView/组件挂载：`apps/host/src/main/windows.ts`、`apps/host/src/main/plugins/views.ts`。
- Renderer 根：`apps/host/src/renderer/App.tsx:1-12`，同时挂载 `HubShell` 与首次导入 `ImportWizard`。
- 主工作台：`apps/host/src/renderer/components/hub/HubShell.tsx:1-约340`；真实默认 A 方案，B/C 为 URL `?variant=B|C` 比较入口（`useHubModel.ts:5-34`）。三种布局共用模型与生命周期，不是三套业务。
- A：左侧导航 + 中央工作区；主页/应用管理、筛选、搜索、应用列表；应用详情/设置复用中央区域（`HubShell.tsx:134-142`）。
- B：顶部品牌/导航 + 中央命令检索面 + 右侧详情 + 底部状态栏；键盘优先（`HubShell.tsx:155-160`）。
- C：顶部导航 + 工具卡片网格 + 统计摘要/说明 footer（`HubShell.tsx:162-164`）。
- 通用弹层：⌘K 命令面板，搜索应用、应用快捷操作、设置、回工作台；关闭时恢复原生组件 workspace focus（`HubShell.tsx:166-170,194-255`）。详情弹层、overlay 可见性也会通知宿主隐藏/恢复组件 surface。

## 2. 宿主可达能力（迁移必须保留）

| 能力 | 入口/实现 | 备注 |
|---|---|---|
| 商店浏览、安装、下载/校验、错误重试 | `apps/host/src/renderer/pages/StorePage.tsx:9-100`; `apps/host/src/renderer/stores/plugins.ts`; `apps/host/src/main/plugins/installer.ts`, `registry.ts` | 真正可达；安装不等于挂载 |
| 已安装列表、启用/停用、卸载 | `InstalledPage.tsx:7-62`; `plugins.ts`; `component-operations.ts` | 停用保留明确挂载意图；卸载清理意图 |
| 应用详情/打开、懒加载 main、挂载/卸载 | `HubShell.tsx:36-45,100-132`; `useHubModel.ts:104-150`; `component-runtime.ts`, `lifecycle.ts`, `views.ts` | `active = mounted && enabled`；首次显式打开才 mounted |
| 启动恢复 | `apps/host/src/main/plugins/component-runtime.ts`; `component-mount-intent.ts`; `component-operations.ts` | 老配置无 `mountedIntent` 默认不自动挂载；Run 被暂缓排除 |
| 全局快捷键录入/注册/注销 | `apps/host/src/renderer/components/ComponentShortcutsSettings.tsx`; `component-shortcuts.ts`; `packages/plugin-sdk/src/shortcuts.ts`; `preload/host.ts:8-13` | 仅已安装、挂载、启用且启动成功的组件注册；冲突保留旧值 |
| 快捷键唤起模式 | `SettingsPage.tsx:24-115`; `stores/settings.ts`; `shortcuts.ts`; `types.ts:20-70` | `app-only`（只显示应用）/`launcher`；下一次快捷键生效，不重启实例 |
| ⌘K 命令面板/组件 feature 命令 | `component-command-palette.ts`; `component-features` hook; `component-dispatch` handlers; `HubShell.tsx:144-169` | dispatch 经过活动组件 gate，不会自动挂载 |
| 原生应用菜单/菜单栏设置 | `apps/host/src/main/workspace-menu.ts:1-18`; `component-menu-bar.ts`; `component-shortcuts.ts` | 动态 Workspace 菜单、组件贡献菜单项；迁移到 GPUI 要映射到 native menu/command surface |
| 主窗口控制 | `preload/host.ts:12-13`; `ipc/desktop-handlers.ts`; `windows.ts` | minimize/maximize/close；macOS close 隐藏而非退出（README 约 100-110） |
| 数据目录显示、迁移、采用新空目录、恢复备份 | `SettingsPage.tsx:86-110`; `DataDirRow.tsx`; `MigrateDialog.tsx`; `storage/migrate.ts:58-213`; `component-migration.ts` | 目录白名单、非空拒绝、staging/备份/回退/指针切换；不可简化为改路径 |
| 主题 | `SettingsPage.tsx:43-66`; `stores/settings.ts`; `hub-tokens.css`, `hub.css` | light/dark/system；组件 preload 注入全局 CSS |
| 首次旧数据导入 | `ImportWizard.tsx:7-113`; `stores/legacy-import.ts` | 扫描 2FA/Marks/Monitor，明确跳过或确认；焦点圈、Esc、读屏反馈 |
| 外部 MCP | `apps/host/src/main/plugins/component-mcp.ts`; `component-mcp-socket.ts:1-250`; `scripts/component-mcp-stdio.mjs`; `preload/host.ts:8` | 私有 Unix socket、活动组件 gate、大小/连接/并发/超时限制；停用组件不可被 MCP 自动挂载 |
| 插件 SDK IPC | `packages/plugin-sdk/src/ipc.ts`, `runtime.ts`, `goose.ts`; `apps/host/src/preload/plugin.ts:1-85` | storage、clipboard、shell open、notification、subInput、lifecycle、focus/visibility、MCP |
| 安全边界 | `windows.ts`; `README.md` MCP 段 | contextIsolation=true、sandbox=true、nodeIntegration=false；秘密不进入通用日志 |

## 3. 四个组件的业务能力边界

### 2FA（真正 Hub 组件）
Manifest：`components/2fa/manifest.json:1-60`。OTP/TOTP 计算、账户分组、快速取码并粘贴、剪贴板识别导入、屏幕二维码扫描、多格式导入、删除/回收站、数据迁移、安全存储与 safeStorage、MCP vault；入口 `components/2fa/main/index.ts`、`src/main.tsx`，UI 主要在 `src/components/{AccountDetail,AddAccount,CodeGrid,QuickCode,DataTransfer,TrashBin}.tsx`，密码/密钥逻辑 `main/twofa-*.ts` 与 `src/lib/{otp,data-transfer,google-auth-migration}.ts`。迁移到 GPUI 必须保留安全存储、剪贴板/通知/对话框权限与 OTP 生成，不只复刻列表。

### Marks（真正 Hub 组件）
入口：`components/marks/main/index.ts`、`src/hub-entry.ts`、`src/main.tsx`。书签新增/编辑/删除、分组与排序、模板 URL `{query}`、万能搜索/回车打开、导入导出、图标缓存/失败回退、死链探测、附件、同步安全、MCP；AI 侧含 OpenAI Responses、兼容 Chat Completions、Anthropic 路径（`src/constants/ai.ts`, `src/lib/aiProvider.ts`, `src/hooks/useAI.ts`）。浏览器当前 URL 与 uTools 内建 AI/用户资料不可在 Hub 伪造（`COMPONENT_STATUS.md`）。UI 布局入口 `src/views/home/{HomePage,SidebarNav,AddBookmarkWizard,GroupManagePage,AvatarMenu,HelpAboutDialog}.tsx`。

### Monitor（真正 Hub 组件）
入口：`components/monitor/main/index.ts`、`packages/core/src/{entry,main}.ts`。按应用归并 Electron/Chrome Helper，左右键展开 GPU/标签/网络服务；搜索端口并回车终止整组；可见窗口分类；实时网络上下行；PID 复用二次校验；进程、窗口、端口、网络 provider；偏好持久化；MCP 与 owned-processes。布局/语义由 `packages/core/src/category-layout.ts`, `responsive-layout.ts`, `product-surface.test.ts` 及 `styles/app.css` 定义。

### Run（源码在 Hub，但当前产品路径有明确缺口）
Manifest：`components/run/manifest.json:1-31`；入口 `main/index.ts`、`src/App.tsx`、`src/components/TerminalPanel.tsx`、`crates/goose-run`、`native/addon.mm`。脚本库 CRUD、分组、搜索、运行/停止、cwd/env/shell/确认运行、真实 PTY、xterm/libghostty/native resize/粘贴/IME、多会话与 `window.json` 语义。README 明确完整终端仍是独立 GooseRun.app，Hub 当前是非完整终端/暂缓使用；宿主 `deferredReason` 阻止安装/启用/恢复（`useHubModel.ts:104-123`; `registry/registry.json`）。本次 Zed terminal+GPUI 复刻必须决定是否把 Run 从“暂缓/独立 Tauri”提升为首要集成，并保持脚本数据字段兼容。

## 4. 数据、布局与隐蔽约束

- Host 配置：`~/.config/goose-hub/config.json`；组件目录分别 `~/.config/{2fa,marks,monitor}` 与 `~/.config/goose-run/scripts.json`，环境变量覆盖；路径解析/遍历/重叠拒绝：`apps/host/src/main/storage/paths.ts:9-133`。
- 原子 JSON envelope、每组件串行写队列、watcher、schema 校验、legacy-run 保留额外字段：`storage/json-repository.ts:16-211`、`types.ts:1-74`。
- 停止顺序很关键：abort → 组件 stop → PTY/native/sampler/watcher/MCP/network/listener 释放 → renderer onStop 最后保存；保存失败保留 writer/队列，不能谎报成功（README 生命周期段；`component-runtime.ts`）。
- 组件 surface 与宿主 overlay 共享可见性/焦点 token；命令面板关闭时必须恢复终端/编辑器焦点，不能只关 GPUI modal。
- Accessibility：现有 UI 使用 aria label/live region、键盘箭头/Enter/Escape、焦点圈与 reduced-motion；迁移保留语义反馈而非只用颜色（`ImportWizard.tsx:19-67`; `HubShell.tsx:166-170`; `components/marks/PRODUCT.md`）。

## 5. 死代码、暂缓与风险

- A 是正式主界面；B/C 明确为比较预览，不应被误当成独立产品功能（`useHubModel.ts:16-34`、README 三款主界面）。
- `InstalledPage.tsx`、`StorePage.tsx`、旧 `PluginWorkspace` 等仍是可复用页面/路径，但 A 的 HubShell 已把主要导航/详情重排进自身；迁移前应按调用图分类，不能按文件名认为全部可达。
- Run 的 Hub 嵌入能力被 `deferredReason`/registry 暂缓，独立 Tauri/libghostty 入口仍保留；这是产品状态而非删除代码。
- uTools 专属能力（Marks current-browser、uTools AI/profile；2FA/Marks uTools adapter）在 Hub 下不可达，不能移植成假服务。
- 开发预览 fixture（`useHubModel.ts:64-78` 的无 desktop fallback、`?state=empty`）仅预览死路径，不是运行时能力。
- 外部事实/云同步/真实 AI 与 MCP 客户端完整验收未完成；源码存在不等于当前可交付能力。

## 6. 对 Zed + Rust + GPUI 的推荐保留分层

1. Rust/GPUI 只替换宿主壳与 Hub 布局层；Herdr 底层引用、IPC/协议、组件业务与数据 schema 不改。
2. 以 A 的左导航+中央工作区为正式默认；吸收 B 的 command palette/键盘效率与 Run 的终端主表面，不采用 C 的卡片网格作为高频主路径。
3. 首批必须实现：组件 catalog/install/enable/disable/uninstall、显式 mount intent、焦点/overlay、全局快捷键、原生菜单、设置/迁移/恢复、MCP gate、完整退出保存；这些都是宿主能力，不是视觉可选项。
4. GPUI 视觉按 native-design 的 GPUI 分支采用扁平、紧凑、键盘焦点清晰的 Delta 语言；不要套用 Web 卡片/玻璃拟态，也不要把 Electron 的 CSS 常数直接搬到 Rust。
5. Run 需单独列为架构决策：GPUI terminal surface 是否接管现有 PTY/libghostty，以及如何保持 `scripts.json`/`window.json`/IME/resize/粘贴语义；在此决策前不能声称“功能不变”。
