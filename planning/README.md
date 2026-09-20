# Goose Herdr → Rust + GPUI 规划交付

**当前实施入口：[gap-fill-plan.md](gap-fill-plan.md)。** 对照 goose-herdr 的 Agent 设置、Space/会话、快捷键与 Herdr 契约缺口。迁移前选型快照仍见 [final-plan.md](final-plan.md)。

- 推荐 **S1 + A + TA**；S2全Hub合并、B/C布局、TB直接复用Zed源码均保留候选，不代替用户批准。
- Herdr daemon/CLI/协议与三路PTY所有权不变；Rust协议适配不直接链接SwiftKit。Herdr binary未pin，Zed commit仅研究基线，目标无Cargo构建结果。
- 按用户约定，编译成功即实现验收通过；本轮只验证规划链接与原型逻辑，不冒称Herdr/runtime/IME/VoiceOver通过。

## 入口

- [缺口补齐计划](gap-fill-plan.md)：相对 goose-herdr 的当前功能缺口与分阶段修法。
- [最终整合方案](final-plan.md)：全部能力映射、决策、1-All、两技术路线、迁移/回滚/依赖/风险。
- [三版交互原型](prototypes/index.html)：顶部A/B/C选择；A推荐；固定演示数据，无真实连接。
- [6 Pro完整原文](chatgpt-6pro-review.md) · [真实执行与X调研记录](external-review.md)：已完成一次脱敏复核，浏览器空间已关闭；外部建议不是指令。
- [当前完成审计](completion-audit.md) · [复核历史及最新关闭状态](review-findings.md)。
- [Hub能力扫描](capability-audit.md) · [Herdr契约](herdr-contract.md) · [技术研究](stack-research.md) · [原生设计](native-design.md)。
- [前序计划](full-plan.md) · [原型验证记录](prototypes/verification.md) · [原型检查脚本](prototypes/check.mjs)。

S1/S2、A/B/C与TA/TB等待用户选择；这不阻碍研究交付完成，也不授权业务实施或真实数据迁移。
