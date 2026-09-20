# Goose Hub → GPUI Rust Terminal 原生设计研究报告

## 结论

推荐 **A：高保真工作台**。以 Goose Hub 已有“插件/工作区/菜单栏/快捷键/主题”心智模型为信息架构，迁移 Delta 的扁平表面、细分隔线和中性选中行；终端是主内容，不另造业务入口。B、C 可作为同一布局在窗口宽度/用户偏好下的响应变体。所有现有功能（项目/工作区、插件、命令面板、全局搜索、设置、主题、快捷键、菜单栏、终端输出）必须在侧栏、工具栏、命令面板或设置树中可达。

## 证据边界

- 参考原图已在本地打开并观察：`gpui/references/light-main.png`、`dark-settings.png`。浅色主界面可观察到左侧项目/线程导航、中间终端/内容、右侧文件树，顶部工具栏和底部紧凑状态区；深色设置可观察到左侧搜索+树形导航、右侧标题/分组/扁平设置行和右对齐控件。
- `light-command-menu.png`、`light-project-picker.png`、`light-search*.png`、`light-context-menu.png`、`dark-search.png`、`dark-main-project-picker.png`、`dark-font-settings.png`、`dark-model-picker-annotated.png` 为视觉参考。静态图不能证明 hover/pressed/焦点事件、动画时长或持久选中语义；蓝色大矩形标注不是 UI。
- `dark-font-settings.png` 只证明截图当时 UI=16、Prose=15、Code=15、字体=Default，不能推断工厂默认字体或字体文件。
- 参考路径与规则：`/Users/eachann/Library/Mobile Documents/com~apple~CloudDocs/00000/LinkConfig/agents/skills/native-design/SKILL.md`、`gpui/guide.md`、`gpui/tokens.md`、`gpui/references/README.md`、`sources.md`。
- Goose Hub 祖先规则：`/Users/eachann/Work/goose-hub/AGENTS.md`。它要求复用现有 token、输入焦点仅改边框颜色、不改变几何，并保留文本选择、拖拽排序和键盘焦点反馈。
- Goose Hub 现状证据：`packages/ui/src/tokens.css` 已有 `hub-background/surface/inset/foreground/muted/border/primary/accent/focus` 语义桥、字体/字号 token、`titlebar-height:38px`、`dock-width:64px`；`plugin-global.css` 保留 `:focus-visible`；BoardUI 已有菜单、图标、Select、Switch、快捷键与主题 API；主进程主题支持 `light/dark/system`，插件有焦点/命令面板/快捷键 IPC。

## 目标结构

### A：高保真工作台（推荐）

- 有界侧栏：项目/线程/插件或工作区树；当前项中性灰底，祖先不全部高亮。
- 中央主区：终端输出/输入优先，顶部保持紧凑工具栏，底部状态/操作区；终端可滚动、复制、选择、输入法组合。
- 右侧按需窗格：文件、上下文、运行状态；窄窗优先折叠右侧，不压扁终端输入。
- 浮层承载项目选择、命令面板、搜索和菜单；锚定触发点、限高滚动、Escape 关闭并恢复来源焦点。

### B：终端专注

- 默认隐藏或窄化侧栏/右窗格，以终端最大化；所有功能通过可发现的命令面板、快捷键和临时侧栏可达。
- 适合长时间日志/交互；不把“隐藏”误做删除，保留明显展开按钮和菜单入口。

### C：多任务分栏

- 中央区支持两个或多个终端/线程分栏，分隔线可拖拽且有键盘替代操作；活动 pane 用中性边界/标题标识。
- 窄窗自动降为单 pane，保留 pane 切换与关闭动作。

## 语义令牌建议

沿用 Goose Hub token，不新增平行主题系统；GPUI 映射如下：

| 语义 | 浅色趋势 | 深色趋势 | 约束 |
|---|---|---|---|
| content | `#FAFAFA` | `#1A1C1F` | 主内容，不纯黑 |
| sidebar | `#EBEBEC` | `#212327` | 导航层 |
| surface-raised | `#EBEBEC` | `#23252A` | 菜单/搜索 |
| row-active | `#DADBDD` | `#2D2F34` | 中性灰，不用紫色铺满 |
| primary text | 现有高对比深色 | 约 `#DDE0E4` | 终端和正文优先 |
| secondary text | 现有中灰 | 约 `#AAAFBB` | 路径、说明、快捷键 |
| separator/border | 邻表面略深 | 邻表面略亮 | 低对比、像素对齐 |
| accent/focus | 克制蓝紫 | 克制浅蓝 | 仅焦点/插入点/少量强调 |

状态必须分开命名：normal、hover、pressed、selected、focused、disabled、error/warning/success；状态不能只靠颜色表达。

### 密度、字号和尺寸起点

- 间距：4/8/12/16/24/32；图标与标签 6–8；图标通常 16，命中区由行/按钮容器提供。
- 菜单/树行最小 28–32；紧凑控件约 28–32；设置主区边距 24–32；分节 24–32；标题到内容 12–16。
- 设置行上下留白 12–16，有说明时随文本增长；小控件圆角 4–6，浮层 8–12；整页不做大卡片。
- UI 16 / prose 15 / code 15 只能作为可配置起点；中文 fallback、基线、行高必须实测，字号放大时命中区和行高随内容增长。

## 交互、焦点、键盘与可访问性

- Action 按焦点上下文处理：最内层控件先消费事件，根视图不得截获文本编辑快捷键。Tab/Shift-Tab/Space、方向键、Enter、Escape 遵循控件语义。
- 搜索/命令面板打开即聚焦输入；上下键移动活动行，Enter 执行；关闭恢复触发来源焦点。菜单支持子菜单方向键；普通浮层不默认锁死全窗焦点，模态对话框才约束焦点。
- 输入必须支持插入点、选择、复制粘贴、撤销、中文 IME 组合；组合文本未提交时 Enter 不得误执行命令。placeholder 不是持久 label。
- 纯图标必须有可访问名称，必要时 tooltip 提示动作/快捷键；截断标签保留完整 accessible name。提供 role、name、value/state、enabled/focusable；不能声称 GPUI 自动继承 SwiftUI 无障碍能力，必须核对项目实际平台层与测试结果。
- VoiceOver 风险：GPUI 自绘元素的语义树、焦点通知、动态终端内容朗读、菜单/对话框焦点恢复都可能需要平台桥接；在没有真实 macOS VoiceOver 验证前只能标为“待验证”，不可宣称已支持。
- Reduce Motion：所有过渡采用短、克制的 opacity/颜色/尺寸变化；关闭减少动态效果时跳过位移、弹跳和持续动画，状态仍可由颜色、形状、文本、图标识别。不要从静态截图推导动效。

## 动画与窗口行为

- 仅浮层真实叠放使用短柔和阴影；主结构靠表面和细分隔线。
- 菜单/搜索面板避免弹跳和大位移；侧栏折叠、分栏变化可用短过渡，reduce motion 下直接切换。
- 标题栏保留平台拖拽、双击和系统按钮；不绘制装饰性红绿灯。
- 使用逻辑单位，不把截图设备像素当常数；细线按目标缩放像素对齐。

## 完整状态矩阵

| 表面/控件 | normal | hover | pressed | selected/active | focused | disabled | loading/empty/error |
|---|---|---|---|---|---|---|---|
| 侧栏树行 | 普通文字 | 轻微表面变化 | 短暂压下 | 中性灰底+文字 | 键盘焦点可见，不改几何 | 降低对比且不可激活 | 无项目/失败需文字说明与重试 |
| 终端 pane | 输出可选 | 可见可操作区域 | 点击反馈 | 标题/边界标活动 pane | 插入点/焦点标识 | 只读明确说明 | 连接中/断开/权限错误明确 |
| 搜索输入 | label+placeholder | 边框颜色 | 保持几何 | 有查询值 | 光标/键盘反馈 | 不可编辑说明 | 加载、无结果、失败明确 |
| 命令/菜单行 | 图标+名称 | 中性高亮 | 执行反馈 | 当前活动行/勾选 | 键盘活动行 | 灰化且不执行 | 无匹配/分组标题 |
| 按钮/图标按钮 | 中性小圆角 | 表面变化 | 压下 | toggle 状态 | focus-visible | 不可点击 | 执行中/失败反馈 |
| 设置行 | 扁平、单分隔线 | 不铺卡片 | 控件反馈 | 当前页或当前值 | 控件焦点 | 保留值且说明原因 | 异步保存/错误状态 |
| 分栏分隔线 | 细线 | 命中提示 | 拖拽中 | 当前 pane 标识 | 键盘可调 | 不可调整说明 | 窄窗降级单栏 |

## 实施风险与验证门槛

1. **IME/VoiceOver 高风险**：先做真实 macOS 输入法组合、中文提交、复制/撤销、Tab/Escape/命令面板焦点恢复；再做 VoiceOver role/name/state/动态更新检查。缺少平台桥接时记录为阻塞，不以截图代替。
2. **主题与 token 漂移**：只映射 Hub 已有语义 token；验证 `light/dark/system` 切换、对比度、终端 ANSI 色与错误/警告图标同时可辨。
3. **快捷键冲突**：复用 Hub 现有 shortcut IPC 与 action 上下文；全局监听不得抢文本输入；验证快捷键录制、冲突、禁用与恢复。
4. **窄窗/高缩放**：逻辑尺寸自适应；验证字号放大、长中文、RTL/混排、长路径截断和分栏降级。
5. **动画**：实现后验证系统 Reduce Motion；静态参考仅作视觉依据。
6. **资产许可**：Delta 截图含私人文字，只用于本地参考，不打包、不上传；新增图标优先项目已有图标，若采用 Zed/Lucide/Phosphor，记录具体文件、版本、来源和许可证，不复制整套应用资产。

## 官方来源核验状态

`native-design/sources.md` 指向 GPUI 官方入口、Zed 官方源码/key bindings/platform/element/theme 文档以及图标来源。当前环境对这些外部检索未返回可用结果，因此本报告不把在线主分支 API、版本或许可细节当作已核实事实；实现前应按当前 `Cargo.toml/Cargo.lock` 锁定版本逐项核验。项目验收按祖先 AGENTS：编译成功即可；本报告不声称视觉、IME 或 VoiceOver 已验收。
