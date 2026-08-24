# VeriSilo Agent routing

## 默认读取路径

Camoufox / Managed Identity 普通任务先读：

1. [Camoufox 当前状态](docs/camoufox-program-status.md)中的“当前下一任务”；
2. 实际 owning code/test；只有状态页明确要求独立 active contract 时才再读取它。

Standard Silo、EngineAdapter 或环境后端任务先读各自 owning 文档/代码；只有它实际依赖
当前 Camoufox Gate 时才读 Camoufox 状态页。

只有任务可能改变产品语义、架构路线或长期能力边界时，才额外读取：

1. [身份平台北极星](docs/identity-platform-north-star.md)；
2. [Camoufox-first 决策](docs/camoufox-managed-engine-decision.md)。

只有在委派复杂工作、创建新 Gate 或审阅外部 evidence package 时，才读取
[Agent 协作协议](docs/agent-operating-model.md)。历史任务合同、旧 run 和 superseded
checkpoint 不属于默认上下文；仅在调查对应事实时按状态页链接读取。

若多个事实源真正冲突，权威顺序是：产品北极星 → 已接受架构决策 → 当前任务合同
→ 当前状态 → 实现与直接证据。历史合同中的旧“当前状态”不覆盖状态页标明的新状态。

## 成本与验证

使用能解决当前不确定性的最低充分流程：

- 文档或局部机械修改：只检查相关引用、格式和 diff；
- 普通代码修复：检查 owning seam/callers，做最小修复和 focused tests；
- 浏览器、引擎构建、证据 claim、发布或难回滚操作：使用冻结输入、直接 evidence 和明确停止条件；
- 产品/架构、安全或数据边界变化：先做显式决策。

不要因为历史阶段曾使用 one-shot、manifest、全量 hash 或完整回归，就把它们复制到不需要
这些保证的新任务。稳定且相关状态未变化时复用既有证据；只有矛盾、新失败或关键输入变化
才扩大检查。详细分级见 Agent 协作协议。

## 不可弱化的产品边界

- Standard Silo 长期保留；近期优先关闭 Camoufox Managed Engine 的真实执行风险；
- Profile、Identity Artifact、Engine、Network Policy 与 Evidence 保持不同生命周期；
- 原生 Windows 专属结论不能由 Linux、WSL 或 Wine 结果替代；
- 不同时扩张 Controlled Chromium、WSL、VMware、Hyper-V 与 Remote；
- 不混用 `configured`、`applied`、`observed`、`verified` 与 `unavailable`；
- 配置声明、测试通过或编译成功都不能冒充尚未取得的 runtime/product Gate。
