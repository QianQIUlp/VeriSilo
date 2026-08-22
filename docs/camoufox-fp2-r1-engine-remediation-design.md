# FP2-R1 Engine Remediation Design Freeze

- 状态：**Proposed（待主脑 Gate 审阅）**
- 形成日期：2026-08-22
- 授权任务：`FP2-R1 Engine Remediation Design Freeze`（offline/static only）
- 浏览器执行：本任务全程未启动浏览器；Browser 保持 CLOSED

本文冻结 GPC 与 voices 在当前 v152 candidate 内的精确 source seam、单一事实源设计方向、
DNT Policy V-next 版本化合同，以及 R1 fresh lineage 与 FP1 carry-forward 资格规则。
它不批准任何实现补丁；补丁实现属于后续独立合同。

## 绑定锚点

```text
source lock:
apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-canvas-v1-source.json
(lockSha256 42b14bfe7331e6c023a3a6fa49da614852b6fd0a28c3225cc84bfc515d4036d5)

upstream:
https://github.com/daijro/camoufox
tag v152.0.4-beta.28
commit 0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d
tree   1435d544d9b61dee7fcf74cf92462952ca43d38e
（本次分析以 sparse checkout 取得该 commit，git 根树哈希与 lock 逐字节一致）

Firefox 基线对照:
archive.mozilla.org firefox-152.0.4.source（sha512 见 lock）
逐文件比对使用 hg.mozilla.org releases/mozilla-release @ FIREFOX_152_0_4_RELEASE

patch 应用实况:
artifacts/camoufox-fp1/windows-candidate-20260818T061456Z-e571f6c/
provenance/container.log（50 个上游 patch 全部成功，无 reject；
speech-voices/voice/navigator/fingerprint 各 hunk 记录见下文）
```

术语约束（主脑裁定，本文遵守）：`settings/properties.json` 是 **engine configuration
contract 对属性的识别声明**，不是运行时实现正确性的证明。Gen5 证据证明的正是后一层未兑现。

---

## A. GPC source seam（已完全裁决）

### A.1 Seam 表（v152 candidate 构建内）

| 暴露面 | 文件 / 函数 | 本构建内的机制 |
| --- | --- | --- |
| Window `navigator.globalPrivacyControl` | `dom/base/Navigator.cpp` `Navigator::GlobalPrivacyControl()` | **未被任何 patch 修改**。native 路径：`privacy.globalprivacycontrol.enabled`，false 时回退 private-browsing 分支（`UsePrivateBrowsing() && privacy.globalprivacycontrol.pbmode.enabled`），最终被 `privacy.globalprivacycontrol.functionality_enabled` 门控。三个 pref 均无消费者设置 |
| Worker `navigator.globalPrivacyControl` | `dom/workers/WorkerNavigator.cpp` `WorkerNavigator::GlobalPrivacyControl()` | `patches/fingerprint-injection.patch` hunk（container.log L503 区域）：先读 `MaskConfig::GetBool("navigator.globalPrivacyControl")`，有值即返回；否则走同一 native pref 回退 |
| `Sec-GPC` 请求头 | netwerk 原生路径 | 未 patch，由上述同一组 pref 驱动 |

关键排除证据：

- `patches/navigator-spoofing.patch` 中 `bool Navigator::GlobalPrivacyControl()` 仅作为
  hunk 头上下文注释出现；该 hunk 的实际改动是其后的 `HardwareConcurrency()`。
- 对全部 50 个 patch、`additions/**`、`settings/camoufox.cfg`、`scripts/**` 的全库检索：
  除上表两处外不存在任何 `globalPrivacyControl` / `Sec-GPC` /
  `privacy.globalprivacycontrol` 引用。camoufox.cfg 不设置这些 pref。
- `properties.json` / `camoucfg.jvv` 声明 `navigator.globalPrivacyControl: bool`
  属 contract 层识别。

### A.2 Gen5 观测的结构性解释

配置 `true` 下观测到 worker=`true`、window=`false`、header 缺失。该三元组由单一机制
完整复现：**worker 侧被 patch 成直读 MaskConfig，window 与 HTTP 侧留在 native pref
路径且无人把配置写入 pref。** 这是 realm-specific consumer divergence（单一 consumer
只覆盖 Worker，其余回退 native），不是"两个 writer"。此前"引擎内部存在两个不一致写入方"
的措辞作废。

### A.3 冻结的单一事实源设计

```text
Artifact GPC bool
        ↓
one canonical engine state（MaskConfig 单键，一次解析）
      ↙        ↓        ↘
 Window    Worker    Sec-GPC HTTP
```

冻结要求（R1 补丁设计的验收边界）：

1. `Navigator::GlobalPrivacyControl()` 与 `WorkerNavigator::GlobalPrivacyControl()`
   读同一个 MaskConfig 键；禁止两侧各自维护状态或经不同中间层取值。
2. `Sec-GPC` 发射与两个 getter 由同一状态驱动：允许的实现形态是"配置加载时单点写入
   Gecko pref"或"直接 patch 头发射点读取同一状态"，二者择一；不得同时引入两条通路。
3. 禁止以独立 pref 开关充当伪装通道；pref 只能是 canonical state 的投影。
4. R1 验证不变量：`window == worker == Sec-GPC-present == 配置值`；
   relation matrix 中 GPC 行保持 managed-required。

---

## B. Voices source seam（seam 已精确定位；根因收敛到初始化/同步时序）

### B.1 已验证的完整链路

```text
resolvedConfig["voices"]（53 条，五字段 {name,lang,voiceUri,isDefault,isLocalService}，
Artifact A 实测形状合法、恰 1 个 default）
→ CAMOU_CONFIG_N（win 每 2047 字符顺序切片；host_v1 normalize_camou_config_env +
  digest 往返校验证明 env 与磁盘 resolvedConfig 逐键等值）
→ MaskConfig::GetJson()（additions/camoucfg/MaskConfig.hpp：顺序拼接、json::accept
  校验；若截断则整体退化为 {}——Gen5 worker GPC=true 反证了截断不存在）
→ MaskConfig::MVoices()（五字段 contains 校验，缺失字段 stderr ERROR 并跳过该条）
→ nsSynthVoiceRegistry::GetInstance()（父进程分支：NS_CreateServicesFromCategory 之后
  注入 MVoices → AddVoiceImpl(nullptr, uri, name, lang, isLocal, false) + SetDefaultVoice）
→ SpeechSynthesisParent::SendInit() 快照 + AddVoiceImpl 内 SendVoiceAdded 增量
  （FF152 新增 InitialVoicesAndState 快照路径；快照与增量均不过滤 null-service 条目；
   GetVoiceCount/GetVoice 为裸 mVoices 枚举，同样无过滤）
→ 内容进程镜像（RecvInitialVoicesAndState / RecvAddVoice）
→ SpeechSynthesis::GetVoices()
```

旁路机制确认：

- `patches/speech-voices-spoofing.patch` 新增 dom/base/SpeechVoicesManager 与
  `window.setSpeechVoices(DOMString)` WebIDL：这是按 userContextId 的**白名单过滤**开关，
  只有调用过才生效；全库（patches + additions）零调用方，VeriSilo 从未激活 ⇒ 过滤恒不参与。
- FF152 Windows 后端位于 `dom/media/webspeech/synth/windows/SapiService.cpp`（旧 `win/`
  目录已不存在）；其枚举 URI 方案 `urn:moz-tts:sapi:<raw name>?<lang>`（L307）并经
  `registry->AddVoice(this,...)`（L315）注册——正是被
  `voices:blockIfNotDefined` 守护的必经函数。"SAPI 绕过守护"分支被否定。
- container.log：`voice-spoofing.patch` 4 个 hunk 全部成功（offset -2/-4，无 fuzz）；
  `speech-voices-spoofing.patch` 成功（moz.build fuzz 1、nsGlobalWindowInner.cpp
  hunk#2 fuzz 1 offset 444 等）。机械应用层面全部落地。

### B.2 Gen5 raw 证据带来的裁决修正（supersedes 第一轮离线解读的部分表述)

A1 `raw-realms.json` 逐 realm 实测：

```text
top-window:          5 条 —— 全部原生 raw-name URI（isDefault 均 false）
same-origin-iframe: 58 条 —— 5 原生 + 全部 53 条配置 slug URI
                              （slug David 带 isDefault:true，与 Artifact A 一致）
cross-origin-iframe: 58 条 —— 同上
```

由此确立的新事实：

1. **注入链路端到端有效**：MVoices 解析、AddVoiceImpl 注册、IPC 同步全部工作。
   第一轮报告中"voices projection 完全失效/零注入"的推断在此修正为
   "注入有效，但枚举一致性失效"。第一轮裁决结论（FP2 Failed、真实 capability gap）
   不变；comparator 按 per-realm 与配置清单全等比较，58 条状态同样不通过，判定不变。
2. **原生语音未被阻断的原因已定位为 wiring 决策而非缺陷**：VeriSilo v3 Artifacts
   （实测 A）不含 `voices:blockIfNotDefined` / `voices:fakeCompletion` 键，
   守护条件永假，SAPI 原生清单必然并入。
3. **真实引擎缺陷（新精确定义）＝枚举一致性与默认值语义**：
   - 同一会话内 top-window 与 iframe 观测数量不同（5 vs 58）；
   - `isDefault` 跨条目/realm 不一致（原生条目恒 false；配置 default 仅部分 realm 可见）。

### B.3 根因候选（收敛后，全部属父进程↔内容进程语音传播的时序/初始化域）

| 编号 | 候选 | 判别实验（R1 instrumented build，offline 准备、运行需未来授权） |
| --- | --- | --- |
| V1 | 父进程首次 `GetInstance()` 懒触发晚于首个内容进程绑定：快照仅含当时已有条目，注入批次随后以增量到达 | 浏览器进程 stderr 时间戳（printf_stderr 打点 GetInstance/注入/AddVoice/SendInit 序列） |
| V2 | SAPI 异步枚举与同步注入交错：先注后原或先原后注的不同交错产生不同中间态 | 同上打点 + 受控延迟查询序列 |
| V3 | `mVoiceCache` / per-window 状态与 patched GetVoices 的交互 | 双次查询对比（同 window 间隔查询是否收敛） |
| V4 | fuzz=2 下 hunk 落点语义漂移（机械成功但位置偏移） | 对构建产物做源级 diff 审计（R1 新 lock 必含逐文件 post-patch digest） |

在 V1–V4 收敛前不批准任何实现补丁（维持主脑指令）。

### B.4 冻结的单一事实源设计

```text
MVoices（父进程启动期 eager 解析一次）
        ↓
one canonical voice inventory（受管清单存在 ⇒ 原生清单阻断）
      ↙         ↓         ↘
 快照       增量广播     SpeakImpl fake-completion
（三通道必须呈现同一有序清单与同一 default 语义）
```

冻结要求：

1. 清单构建必须发生在首个内容进程可绑定的时刻之前（eager），消除懒初始化窗口；
2. `voices:blockIfNotDefined` 语义改为"managed 清单声明即隐含阻断"，同时 R1 Artifacts
   显式携带 `voices:blockIfNotDefined=true`、`voices:fakeCompletion=true` 及
   `charsPerSecond`，validator 在 `voices` 存在时强制校验这三键（消除本次的静默缺省）；
3. `isDefault` 唯一来源为受管清单；原生条目不得进入暴露集合；
4. `window.setSpeechVoices` 维持 inert 并在文档标注为非受管实验接口；
5. R1 验证不变量：任意 realm、任意查询时刻，getVoices 与配置清单全等（URI、顺序无关
   比较、default 唯一且指向配置项），worker/iframe/top-window 三方一致。

---

## C. DNT Policy V-next（版本化合同冻结）

```text
policyVersion: verisilo.identity-policy.v4-dnt-native
selection:     fail-closed by binding (engineRevision + ffVersion)
```

| 维度 | FF < 135（legacy 分支） | FF >= 135（native 分支） |
| --- | --- | --- |
| `navigator.doNotTrack` | managed-required（沿用现规则，目标值 `"1"`） | **native/unavailable**：不再声明管理目标 |
| requiredConfigKeys | 含 `navigator.doNotTrack: STR` | **不含**该键；出现该键视为 policy-schema 违例 |
| stableWebsiteFields | 含 dnt 观测字段 | **不含**；移入 nativeSignals 观测段 |
| configured/applied/observed 表达 | 现行三态 | 该信号仅有 observed（native capture），无 configured/applied 声明；comparator 规则"无配置 + 任意观测 = PASS(native)" |
| header relation matrix | DNT 行 managed | DNT 行标 `not-applicable/native`，不参与失败判定 |
| Artifact schema | verisilo-camoufox-resolved-identity/v3 | **v4 变体**（由 policyVersion 选择），消除"policy 说 unavailable 而 schema 强制 STR"的内在矛盾 |

约束：

- 历史 A/B Artifact 不可变；Gen5 在 v3 语义下保持 Failed，不得重解释。
- 未来 R1 使用全新 `Artifact A-R1/B-R1`（new policy generation），禁止把旧 A/B 的
  DNT 改写为 unspecified 后复用。
- validator 按 `policyVersion + ffVersion` 严格二选一语义，禁止模糊兼容。

---

## D. R1 fresh lineage 与 FP1 carry-forward qualification

### D.1 Fresh lineage 结构

```text
Historical lineage（immutable）           Remediation lineage（新建）
  artifacts/camoufox-fp2/                   artifacts/camoufox-fp2-r1/
  claims: fp2-vN-one-shot-claim.json        claims: fp2-r1-*-claim 命名空间
  Gen1–Gen5                                 R1 执行合同另行起草
  Gen6: permanently closed                  Generation 命名永久退役
```

回答的权威问题从此是："哪个 engine revision 在哪个 identity policy version 下第一次通过
FP2？"——而不是 Gen 计数的延续。

### D.2 FP1 carry-forward qualification（引擎重编译后的强制条款)

历史 FP1 Accepted 绑定旧 archive/executable/tree/engine revision；新 R1 二进制**不自动继承**。
新候选必须通过：

1. Canvas deterministic patch/seam 存在于新树（对 0001-verisilo-canvas-export-key.patch
   的目标 seam 做预/后像 digest 校验）；
2. bounded-close patch/seam 存在（0002-verisilo-juggler-bounded-close.patch 同上）；
3. source/build binding 完整（upstream revision、FF source digest、patch 顺序与逐文件
   digest、toolchain、archive SHA/size、tree manifest、BuildID/SourceStamp）；
4. deterministic replay 是否需要完整重跑 FP1 one-shot：待 R1 补丁设计冻结后由主脑决定；
   "无需重新资格验证"这一命题不成立。

### D.3 后续 Gate 序列

```text
本设计冻结 → 主脑审阅
  → 补丁实现合同（新增下游 patch 文件，含 seam pre/post digest 与判别实验打点）
  → 新 candidate build + FP1 carry-forward qualification
  → A-R1/B-R1 生成（Policy v4-dnt-native）
  → FP2-R1 执行合同（浏览器授权回到主脑）
```

## 附：本次分析的证据索引

- 上游源码：temp sparse checkout @ 0583c3ec…（根树哈希匹配 lock）；
  hg.mozilla.org FIREFOX_152_0_4_RELEASE 的 Navigator.cpp / nsSynthVoiceRegistry.{cpp,h} /
  SpeechSynthesis.cpp / synth/moz.build / windows/{moz.build,SapiService.cpp} /
  ipc/SpeechSynthesisParent.cpp。
- 构建实况：candidate provenance/container.log（patch 逐 hunk 记录）。
- 运行证据：fp2-20260822T065118Z-18fdbee7a8/A1/raw-realms.json（voices 逐 realm 数组）、
  Artifact A（tests/fixtures/camoufox/identity-win-canvas-v1-a.json，47-key，
  voices[53] 合法、无 blockIfNotDefined/fakeCompletion）。
