# Camoufox-first Managed Engine 架构决策

- 状态：**Accepted**
- 决策形成：2026-08-03
- 当前事实与 Gate：[Camoufox 状态页](camoufox-program-status.md)

本文只记录稳定架构决策。阶段 checkpoint、失败 run、patch hash 和实施细节属于状态页、
当前任务合同、lock/result 与 Git，不追加到本 ADR。

## 问题

VeriSilo 已有 Silo、Vault、Profile、网络 Provider 和 EngineAdapter 控制面，当前最大的产品
未知不是“能否再做一个控制界面”，而是能否把持久、协调的身份真实应用到网站可见信号并
跨启动重放。个人开发者资源不足以同时维护多个浏览器内核或默认引入整机虚拟化。

## 决策

第一条 Managed Identity Engine 路线采用：

```text
VeriSilo
→ EngineAdapter
→ standalone Host
→ 固定且受绑定的 Camoufox/Firefox engine
→ Resolved Identity Artifact
→ Persistent Profile
→ first-party runtime evidence
```

Camoufox-first 是近期风险顺序，不删除 Standard Silo，也不把 Camoufox 等同于整个产品。
先关闭一个受控引擎的真实执行风险，再做桌面产品集成、网络协调和发布。

## 职责边界

- **BrowserForge** 只生成受约束的候选身份参数，不负责浏览器实际返回值。
- **Camoufox** 在 Firefox 内核层应用身份，不是管理平台，也不是 Chromium 模拟。
- **standalone Host** 负责资产绑定、Artifact 重放、Profile 独占和浏览器生命周期。
- **EngineAdapter** 负责桌面调用、包验证、transport 和能力状态，不重新生成身份。
- **VeriSilo** 负责 Silo、Vault、用户策略、网络、迁移、UI 和 evidence 语义。

Profile、Identity Artifact、Engine Binding、Network Policy 与 Runtime Evidence 保持独立
生命周期。配置声明不能替代 runtime observation，Host 自报不能自动提升为 verified。

## 为什么是 Camoufox

- 它提供 Firefox 内核层信号控制，避免把 JS getter、扩展或 stealth 脚本当成完整身份层；
- 可复用预编译资产与窄 downstream patch，符合个人开发者资源；
- 能先证明“生成 → 存储 → 应用 → 冷启动重放 → 网站观测”的最小垂直链；
- 比同时自研 Chromium、Camoufox 与虚拟化后端更可维护。

## 平台分工

- **原生 Windows**：Windows 资产、桌面生命周期、文件锁、Job Object 与 Windows 专属结论；
- **Linux/构建宿主**：开发、静态验证和按需 engine cross-build，不替代 Windows runtime；
- **Tauri/EngineAdapter**：只消费已通过对应 standalone/runtime Gate 的能力；
- **临时构建机**：一次性编译工具，不是产品架构或长期事实源。

## 当前不采用

| 路线 | 决策 |
| --- | --- |
| 只继续打磨 Standard | Standard 保留，但不替代 Managed Engine 风险验证 |
| Controlled Chromium | 延后；出现明确 Chromium 兼容需求和可承担维护资源时重评 |
| BrowserForge 单独作为执行层 | 拒绝；它不改变浏览器实现 |
| JS/扩展 stealth 作为主身份层 | 拒绝；只允许有限实验或观测 |
| WSL/VMware/Hyper-V/Remote 作为默认身份层 | 延后；环境隔离不等于协调身份 |
| 同时维护多个受控内核 | 拒绝当前并行投入 |

## 版本与发布原则

- 固定经过验收的 engine、source revision、patch series、toolchain 与资产；
- 不在 runtime 跟随 `latest`，不把自动下载当作安装策略；
- 自建 archive 不冒充上游 release；身份声明版本必须与真实 engine binding 一致；
- 上游升级只验证受影响能力，不自动重跑所有历史研究；
- build/provenance、runtime qualification、签名发布与产品 shipped 是不同 Gate。

## 当前执行顺序

```text
完成当前 Camoufox FP2-R1 remediation
→ FP1-R1 rebuilt-engine carry-forward
→ FP2-R1 cross-realm/replay qualification
→ FP3 network/geo/timezone/locale coordination
→ FP4 ordinary-site compatibility
→ 使用最终 Managed Engine 重开 clean M3-WI
```

精确下一任务以状态页为准。旧 M3-WI、FP1/FP2 generation、diagnostic build 与 one-shot
执行历史只在调查对应 evidence 时读取，不构成新任务的默认流程模板。

## 重评或变更条件

以下情况需要新的显式架构决策：

- 主要用户或目标站点必须依赖 Chromium API/扩展生态；
- Camoufox 无法满足明确兼容目标或停止维护；
- 产品需要直接控制 Chromium TLS/QUIC/V8 特有行为；
- 团队和收入足以承担另一条固定内核的长期 patch/build/upgrade；
- 要把 WSL、VM、Remote 设为普通 Silo 默认执行层；
- 要合并 Profile、Artifact、网络秘密或 evidence 生命周期；
- 要弱化 `configured/applied/observed/verified/unavailable` 的区别。

局部 bug、patch、builder 差异或测试失败不自动重开架构；只修复直接 owning seam，并把
证据能支持的结论限定在对应层。
