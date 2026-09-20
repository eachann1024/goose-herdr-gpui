# 原型验证记录（2026-09-18）

- `node --check /tmp/proto.js`：通过，确认内联 JavaScript 可解析。
- `node planning/prototypes/check.mjs`：通过，确认 A/B/C 标记、ARIA、流程图、模拟标记与 console.assert 存在。
- 静态检查：未发现 iframe、fetch、XHR、WebSocket、window.open 或外部 URL；所有数据固定在本地 HTML，附件仅展示文件名，不上传。
- 本轮修复：C 新增可见 planning pane；发送按钮/Enter 追加模拟输出；通知弹出模拟列表；附件显示本地假文件名；远程连接更新 simulated 状态；命令面板搜索筛选。
- 未覆盖：未启动真实 Herdr、未连接远程设备、未做视觉截图或辅助功能验收；关闭确认仍使用浏览器 confirm，仅作为原型演示。

## 最终 P2 修复（2026-09-18）
- 加入 `prefers-reduced-motion: reduce`，禁用过渡与平滑滚动。
- pane 增加 `role=region`、具体 `aria-label`、`tabindex=0`；关闭/主题/命令/设置按钮增加可访问名称。
- C 增加原生 range 分栏比例控件，支持拖动和键盘调整。
- `check.mjs` 使用 Node `vm` 执行独立布局、搜索过滤、发送输出逻辑断言；同时包含 `node --check` 可重复语法检查。
- 未声称浏览器视觉或真实 Herdr 验收；仍为无网络本地交互原型。
