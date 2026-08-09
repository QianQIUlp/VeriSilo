# VeriSilo Agent routing

涉及 Silo、浏览器身份、Camoufox、EngineAdapter、环境后端或产品路线的工作，必须先按顺序阅读：

1. [`docs/identity-platform-north-star.md`](docs/identity-platform-north-star.md) — 长期产品意图；
2. [`docs/camoufox-managed-engine-decision.md`](docs/camoufox-managed-engine-decision.md) — 当前 Camoufox-first 路线及被延后的方向；
3. [`docs/agent-operating-model.md`](docs/agent-operating-model.md) — 主脑与执行 Agent 的分工和 Gate 规则；
4. [`docs/camoufox-program-status.md`](docs/camoufox-program-status.md) — 当前 checkpoint、阶段和下一任务。

权威顺序是：产品北极星 → 已接受架构决策 → 任务合同 → 当前状态 → 实现与证据。状态页不能反向改写长期产品意图，单个执行任务也不能静默改变架构决策。

当前防漂移原则：

- Standard Silo 长期保留，但近期优先关闭 Camoufox Managed Engine 的执行风险；
- Profile、Identity Artifact、Engine、Network Policy 和 Evidence 是不同生命周期；
- M2-W 必须在原生 Windows 完成，Linux/WSL/Wine 结果不能替代；
- M2-W 通过前不接入 Tauri/EngineAdapter；
- 不同时扩张 Controlled Chromium、WSL、VMware、Hyper-V 和 Remote；
- 不把 `configured`、`applied`、`observed`、`verified` 或 `unavailable` 混为一谈；
- 主脑既不能完全不审阅执行结果，也不能默认把执行 Agent 的全部劳动重做一遍。

当前具体 Gate 以状态页为准。若状态页与实现或证据冲突，停止推进并让负责主脑解决事实源冲突，不要自行选择更方便的结论。
