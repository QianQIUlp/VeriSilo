# Camoufox-first Managed Engine 架构决策

- 状态：**Accepted**
- 决策形成：2026-08-03
- 最后确认：2026-08-10

本文记录为什么 VeriSilo 当前优先完成 Camoufox Managed Engine，而不是继续扩张控制面、自研 Chromium，或把虚拟化当成指纹层。

## 背景

VeriSilo 已经具备较完整的 Silo、Vault、独立 Profile、Stock Chrome/Edge、网络 Provider、EngineAdapter 控制协议和环境后端框架。真正没有落地的是：把一套持久、协调、可重放的身份实际应用到网站可见浏览器信号的执行层。

### 第一次主脑纠偏：产品顺序不等于当前风险顺序

最初的 Codex 归纳正确识别了 Standard、Managed、Isolated 三层，却把一般性的“先完成基础层”继续当成当前执行顺序，并更多是在压缩对话，而没有充分理解个人开发者最需要验证的未知。用户随后明确纠正：Standard 控制层不是当前最大风险，首先需要证明指纹执行与持久存储能力能否成立。

这项纠正不否定 Standard Silo 的长期价值，而是把近期工作改为风险优先：

```text
已有管理与控制平面
→ 先验证缺失的 Managed Engine 执行面
→ 原生 Windows Gate
→ 再接入既有 EngineAdapter/Tauri
```

## 决策

第一条 Managed Identity Engine 路线采用：

```text
VeriSilo
→ EngineAdapter（M3 接入）
→ standalone Python Host
→ 固定预编译 Camoufox
→ BrowserForge / constrained preset
→ Resolved Identity Artifact
→ Persistent Profile
→ first-party probe evidence
```

职责边界如下：

- **BrowserForge** 生成来自现实分布、相互约束的候选身份参数；它不是浏览器内核。
- **Camoufox** 在 Firefox 内核层应用身份；它不是 VeriSilo 管理平台，也不能宣传为真实 Chromium 模拟。
- **standalone Host** 负责固定资产、Artifact 重放、Profile 独占、浏览器生命周期和严格本地协议。
- **EngineAdapter** 在 M2-W 通过后连接桌面控制面、包验证、bootstrap、receipt 和能力状态。
- **VeriSilo** 负责 Silo、Vault、用户策略、网络、版本迁移、UI 和证据语义。

## 为什么选择这条路线

### 个人开发者资源约束

当前 Linux 开发机为 2C / 8GB / 64GiB，适合运行预编译 Camoufox、少量浏览器实例、Python Host 和 Playwright 证据测试；不适合保存和编译完整 Firefox/Chromium 源码树，更不适合长期维护多平台 patch stack。

### 复用真实内核能力

JavaScript getter 覆盖、扩展和 stealth 插件只能处理部分表面信号，容易产生 Window、iframe、Worker、Header 与引擎行为矛盾。Camoufox 提供更接近所需层级的 Firefox 内核控制，使个人开发者可以把资源放在身份制品、Profile、版本和产品集成上。

### 先证明最小垂直链路

当前必须先证明：

```text
生成身份
→ 存储 Artifact
→ 用固定引擎应用
→ 跨冷启动和 Host 进程保持
→ 由网站探针观测
```

在这条链路成立前继续扩张 UI、虚拟化或更多控制协议，不能回答 VeriSilo 是否真的拥有 Managed Identity 能力。

## 平台分工

- **Linux**：开发 Host、固定资产、生成/重放 Artifact、自动化探针和轻量兼容性测试。
- **原生 Windows**：验证 Windows 资产、Profile 持久化、文件锁、进程句柄、Job Object、reparse point 和真实桌面生命周期。
- **Tauri/EngineAdapter**：只在 Windows standalone Gate 通过后进入集成。
- **按需高配构建机**：仅在未来确实需要维护浏览器源码时使用，不让当前 Linux 主机承担。

## 当前不采用的路线

| 路线                                      | 当前决定                                                       |
| ----------------------------------------- | -------------------------------------------------------------- |
| 继续只打磨 Standard Silo                  | Standard 保留，但不替代当前 Managed Engine 风险验证            |
| 自研 Controlled Chromium                  | 延后；个人开发者当前无法合理承担 patch、构建、升级和多平台成本 |
| BrowserForge 单独使用                     | 拒绝；它生成配置，不负责让普通浏览器真实返回配置               |
| JS stealth / 扩展注入作为专业指纹层       | 拒绝作为主执行层；只可用于有限观察或显式实验                   |
| WSL 作为默认指纹隔离                      | 拒绝；只作为 Linux 浏览器环境，不等于独立设备                  |
| VMware 集成                               | 暂停；会把项目扩张为桌面虚拟化编排器                           |
| Hyper-V / Remote 优先                     | 延后到 Managed Engine 和基础产品稳定之后                       |
| 同时维护 Firefox 与 Chromium 两条受控内核 | 拒绝当前并行投入，先完成一个可用引擎                           |

## 版本与发布原则

- 固定经过 VeriSilo 验收的 Camoufox、Playwright、BrowserForge 和浏览器资产版本。
- 不在运行时跟随 `latest`，不把自动下载当作安装策略。
- 身份声明的浏览器版本必须与真实引擎能力和资产绑定。
- 上游更新经过独立兼容性、Artifact 重放和平台 Gate 后才能进入新版本。
- 当前 Linux/Windows Host 证据不等于可发布引擎包；签名、SBOM、NOTICE、更新和回滚仍属于后续发布 Gate。

## 明确边界

- Camoufox 是 Firefox 系身份引擎；产品必须写成 Camoufox/Firefox Identity，不能写成 Chrome 模拟。
- 当前不声明 TLS ClientHello、QUIC、跨主机字体隔离、Canvas 身份稳定或不可检测。
- `verified: false` 的原型证据不能被 UI 或文档提升为产品验证。
- Resolved Artifact 是内部重放制品，不是最终高级自定义 UI。

## 重新评估 Controlled Chromium 的触发条件

只有出现以下一种或多种情况时，才重新打开浏览器内核路线决策：

- 目标网站或主要用户必须依赖 Chromium 特有 API 或 Chrome 扩展生态；
- Firefox 身份无法满足明确的兼容性目标；
- 产品需要直接控制 Chromium TLS/QUIC 或 V8 特有行为；
- Camoufox 维护停止、许可证/分发条件不再可接受，或关键缺陷长期无法修复；
- 产品收入和团队规模能够支持固定上游版本、patch stack、多平台构建和持续升级。

## M3 桌面接入决策

M2-W 已于 2026-08-09 Accepted。两套现有协议的真实语义不同：generic external-engine 路径是一次性 length-prefixed bootstrap/receipt，standalone Host 是持久双向 JSON Lines 生命周期。M3 因此采用以下接入方式：

- Camoufox package entrypoint 是受验证的 Host，不是浏览器树中的原始 `camoufox.exe`；
- EngineAdapter 为 Host 使用显式、独立的 `camoufox-host-jsonl-v1` transport，Controlled Chromium 的既有 native bootstrap v1 不变；
- v3 Resolved Identity Artifact 的 ID + raw file SHA 是 Camoufox 重放权威，桌面不在启动时重新生成身份；
- Silo seed、Artifact seed 和代理秘密不进入 Host/browser argv、日志或 evidence；
- Host 的 `verified: false` / `observed-on-this-host` 结果只映射为 applied/observed，不伪造成 generic `verify` receipt；
- 先由 M3-0 fake Host contract Gate 关闭协议、包和生命周期接缝，再进行独立的原生 Windows M3-WI；
- 发布 signer、真实 Host runtime/package、UI、代理、site fallback 与 TLS/QUIC 仍属于后续 Gate。

详细冻结范围与验收见 [M3-0 任务合同](camoufox-m3-engine-adapter-task.md)。

## 2026-08-10 执行顺序附录：先收口指纹核心，再重做干净 M3-WI

M3-0 已在 `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd` Accepted，但后续原生
Windows M3-WI 真实 Host 调查没有定位第二 Host 历史 120 秒停顿的唯一调用，
也没有产生 production fix、focused regression 或修复后最终真实验证。因此当前
M3-WI Gate 保持 **Failed**，该次根因调查结论是 **Inconclusive**，Windows
Managed 路径保持 `experimental`。调查收口只绑定到快照
`186484feb935076766beab09595a9270f86f78ef` / tree
`e33d6d68586a79796ffb9bcc668392e369dc97c6`；它不取代 M3-0 Accepted checkpoint。

tracked R2 manifest 仅是执行 Agent 候选 evidence，未被主脑接受；随后的
`186484f` 只增加 R2H runner/freezer/schema 与 Host matrix 调查基础，没有 tracked
R2H result/manifest。不再主动运行 R2/R2H 矩阵追逐偶发失败；若生命周期问题在
后续工作中自然再现，只根据届时持久的协议级阶段/target 证据修复那一个具体调用。

受控引擎近期顺序调整为：

```text
FP1  Deterministic Artifact Projection
→ FP2  top window / iframe / Worker 跨 realm 一致性
→ FP3  代理 IP / Geo / timezone / locale / WebRTC 协调
→ FP4  常见检测站点和普通站点兼容性
→ 使用届时最终 Managed Engine 冻结新的 clean M3-WI
```

FP1 在已成立的 standalone Host 上证明同一 raw Artifact 的确定投影和跨独立
Host 重放；它不宣称 M3-WI 已修复或桌面 Managed 能力已可发布。详细范围见
[FP1 任务合同](camoufox-fp1-deterministic-artifact-projection-task.md)，M3-WI 调查与证据限制见
[M3-WI 历史任务合同](camoufox-m3-wi-windows-task.md)。

这是执行 Gate 的排序调整，不是架构路线变更：Camoufox-first、三层 Silo 并存、
Artifact/Profile/Engine/Network/Evidence 生命周期分离，以及 production package
fail-closed 原则全部保持不变。

## 后果

- 当前 Managed Engine 工程路线只完成一个引擎，不同时扩张其他执行后端；standalone 代码已经进入 `main`，但不表示桌面集成或发布 Gate 已通过。
- M0–M2 已在 Linux 形成可信垂直切片，M2-W 已在原生 Windows 关闭本阶段的平台生命周期差异。
- M2-W 已通过；M3 按上面的专用 Host transport 决策实施，并继续把 production packaging 与真实桌面 evidence 作为独立 Gate。
- 阶段进度与当前 Gate 只在[Camoufox Managed Engine 状态](camoufox-program-status.md)更新，不反向改写本决策原因。
