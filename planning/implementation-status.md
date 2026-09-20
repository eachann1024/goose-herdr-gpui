# S1 实现接线与打包核对

2026-09-18。此表是实现期静态审计，不是运行验收。统一构建任务最终完整 cargo build 已成功，满足用户编译验收。

## 打包

- `scripts/build-app.sh`：正常 Cargo 构建或 `--no-build`；复用固定校验的 UsageHelper runtime；检查非系统动态依赖；ad-hoc 签名；输出独立 `.app`，拒绝覆盖。
- `resources/Info.plist`：独立 `dev.eachann.goose-herdr-gpui`，不冒用旧应用身份。
- `README.md`：构建/用户自主运行命令、原 Tailcat 制品、共享偏好写入风险、未测边界。
- 已执行：`bash -n scripts/build-app.sh`、`plutil -lint resources/Info.plist`、非法参数退出码 2 检查；均通过。
- 最终打包成功：`/Users/eachann/Work/goose-herdr-gpui/dist/final/Goose Herdr GPUI.app`（arm64）。`codesign --verify --deep --strict` 通过；UsageHelper runtime/arm64/node 与 LICENSE 完整；Tailcat 静态链接，otool 输出只有系统库。
- 证据：`dist/final/package.log`、`dist/final/package-verification.log`。没有启动应用、Herdr 或 provider，没有安装覆盖旧应用。

## 已追踪至业务调用的路径

| 用户入口 | 实际接线 |
|---|---|
| 设备选择 / 刷新 | `AppView::connect/refresh` → `transport::connect` → `Client::ping/snapshot` |
| 空间 / tab 创建、重命名、排序、结束 | `submit/dispatch` → mutation；排序已改 `workspace.move_block` / `tab.move` |
| 选 pane / 重连 | `attach` → `attach_command` → `TerminalView::create`；PaneView 保留 Connection guard |
| 本地视图关闭 | 删除 TerminalView / split leaf，不调用 pane.close |
| 结束远端 pane | 确认表单 → `pane.close` |
| 分屏、方向焦点、swap、比例、均分 | actions → `SplitTree` → 递归 render_split |
| Files 浏览、上级、主页、隐藏项 | `load_files` → `files::list_directory` |
| Files 上传/下载/冲突策略/取消 | 表单 → `files::transfer`，CancelToken |
| 附件路径发送 | 后台 `files::attachment_text` → 原目标 TerminalView::paste；目标消失不改投其他终端 |
| 事件订阅 / catalog | `start_events` → Client events/manifests → Worker generation gate |
| 偏好 / Agent启用、路径、顺序 | 设置 action → Settings::save，旧 CFPreferences 域 |
| SSH askpass | main 启动前 handle_askpass → 原 Keychain service；UI新设备 UUID 已修复 |

## 最终接线复核

此前报告的确定缺口已复核修复：

- devices 删除有确认并保留远端会话；DeviceState Drop 显式 shutdown 事件连接，PaneView 保留自己的 Connection。
- inspector 已接 pane.read / send_input / send_keys；session selection 与 retained spaces 有兼容读写、恢复和移除入口，恢复排序使用 move_block，成功创建后排序失败不重复创建。
- workspace/tab 分组、重命名、按钮排序和同设备拖动排序；分栏方向焦点/swap/键盘比例/分隔条鼠标拖动。
- 附件按 manifest 能力区分图片剪贴板和设备路径；本机图片送真实剪贴板，远端附件后台上传，结果仅送原目标终端；Pi 启动就绪轮询与 catalog 门禁已接线。
- 原生菜单统一由 crate::set_menus 设置，包含 Quit/Edit，不再被 AppView 覆盖。
- usage configure/poll/refresh、Cursor显式授权、通知状态转换和点击路由已接线。
- 终端字体/字号/行距和字重/鼠标设置分别通过 set_font/set_style 应用。thinStrokes 保留存储并在设置页禁用说明；旧 renderer 本就忽略该参数，不宣称已有渲染效果。
- SSH密码保留原始空白，Tailcat编辑预填类型；CLI override按旧实现用于catalog过滤/路径显示，不虚构新agent.start协议字段。

此前确定接线错误已通知 owner 并复核修复；这不代表运行验证。语言切换已补齐并重新编译打包：启动恢复和保存时 set_language/重建菜单，界面、设置、搜索、inspector、用量、通知固定文案调用 tr；所有静态 tr 中文条目均在字典找到。用户名称/路径/终端输出不翻译。首包保留，最终产物使用 dist/final。

## 非声明项

未执行真实 daemon/远程连接、真实文件传输、Keychain授权、用量账户访问、通知权限、VoiceOver、IME或视觉运行验收。编译成功不等同于这些项目已验证。S2 Hub 商店与组件宿主不属于本轮 S1。
