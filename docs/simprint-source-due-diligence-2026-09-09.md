# Simprint source-level due diligence

状态：**Accepted product-direction input**。本文记录一次冻结的源码级 review，不是
Simprint runtime qualification，也不是 VeriSilo 当前 baseline 或 release evidence。

## Frozen review objects

| 对象 | Review revision |
| --- | --- |
| Simprint main | `d2dc939bfae8efda8e4e4d791587d7a364ca4dd8` |
| `simprint-browser-kernel` | `simprint/m144`，`9f7b8e0` |
| VeriSilo review baseline | `f6ae647764b2577f2286c1f208c9f02acd4b6fde` |

这些 revision 只定义当时源码审查的输入对象；它们不是当前 VeriSilo canonical baseline。

## What Simprint did better in the reviewed source

Review 中清晰可见的优势包括：

- desktop Profile CRUD breadth，以及 groups、tags、workspaces、recycle-bin 式工作站能力；
- REST API、52 个 MCP tools、per-environment CDP endpoint 和 multi-window input sync；
- account import、extension management、proxy asset usability/health checks；
- updater/release cadence；
- 更广的 Chromium source-patch surface。

其 Chromium patch surface 包含 Camoufox/Firefox 当前不直接匹配的更深 Chromium/net-layer
工作，包括 TLS 相关 patch 与 MAC/network-layer 修改。VeriSilo 不应为了维护自身叙事而贬低
这些真实优势。

## VeriSilo's structural difference

VeriSilo 当前实现的差异不在于“本地而对方不本地”，而在于：

- **Identity Integrity**：Resolved Identity Artifact 版本化、SHA-256 完整性、稳定持久化、
  首次成功启动锁定，并绑定到 engine/runtime；
- **Execution Integrity**：声明的 Artifact 必须由声明且经过验证的 engine/package 执行；
- **Runtime Evidence**：明确区分 `configured`、`applied`、`observed`、`verified`、
  `unavailable`，并将证据绑定到正确 Silo；
- **Network Integrity**：诊断和 evidence 归属于 Silo、provider、selector、node，
  must-proxy 路径在 launch 前 fail closed，不静默降级；
- **Engine/package trust**：exact asset binding、package tree、detached CMS signature、
  Desktop public signer pin 和 fail-closed verification。

`local-first itself is NOT a differentiator`。VeriSilo 的定位不是开源版 AdsPower、免费
商业指纹浏览器克隆或完整浏览器工作站替代品。

## Accepted product verdict

**B — NARROW / DIFFERENTIATE**

这意味着：不停止 VeriSilo，不追求 Simprint/AdsPower 的广度或 feature parity；强化已经
存在、结构不同且用户可感知的 integrity/evidence 能力。该结论不是立即重写架构，也不
移除 Standard Silo；Camoufox-first 仍可作为当前 Managed Engine strategy。

未来大型功能提案默认需要增强至少一项：

1. Identity Integrity；
2. Execution Integrity；
3. Network Integrity；
4. Runtime Evidence；
5. Attribution。

如果功能完全不增强这五项，不能仅因为商业指纹浏览器普遍具备，就自动进入 roadmap。

## Default future non-goals

除非新的具体用户需求重新打开，默认不进入 broad team/workspace/recycle-bin workstation
competition、generic REST API for every resource、huge MCP tool catalog、general RPA、
multi-window syncer、generic proxy asset-management platform、account-import automation、
仅为 feature count 的 Chromium patch arms race，或把 VM/Hyper-V/Remote 变成普通 Silo 执行层。
Windows Sandbox acceptance 是测试实验室证据，不是 VeriSilo 新增 Hyper-V 产品能力的理由。

## Evidence precision

### Fingerprint seed

源码审查发现某 noise seed path 在没有实际 environment/profile id 时 fallback 到
`"default"`。正确结论是：

> Source review found a potential cross-profile fixed-noise correlation defect.

本次 review 没有运行该缺陷的 runtime experiment，因此不能写成“已证明不同 Profile 可以被
关联”。

### Proxy/direct fallback

源码审查发现至少一个 IP/timezone/language detection 辅助路径存在 direct fallback，且没有
发现 VeriSilo 风格的统一 must-proxy launch preflight。正确结论是：

> Simprint did not show an equivalent unified fail-closed launch contract, and at least one auxiliary IP-detection path contains direct fallback.

这不是 Simprint 主浏览器流量已经在 runtime 被证明代理失败后直连泄漏。
