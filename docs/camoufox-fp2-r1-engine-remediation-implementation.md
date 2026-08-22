# FP2-R1 Engine Remediation Implementation Contract

- 状态：**Proposed（待主脑 Gate 审阅）**
- 形成日期：2026-08-22
- 前置：[设计冻结](camoufox-fp2-r1-engine-remediation-design.md) 已 Accepted
  （checkpoint `ff4f8c960d5cf63ff242da990b904fca8743be54` /
  tree `e000f6ad8ebf2bf1ac2c06be8ebdf6fb2c0a4004`），含两项收紧：
  canonical-owner 强制、causal axis 禁判。
- 浏览器执行：**CLOSED**。本合同只冻结规格；构建与诊断实验是后续独立授权步骤。

本合同冻结两套性质不同的改动：

```text
GPC    → remediation patch（产品级，进入 R1 Engine Binding）
Voices → diagnostic instrumentation（调查专用，默认排除于 R1 Engine Binding）
```

禁止把"猜测的 voices fix"与 GPC 修复捆绑进同一正式候选。

---

## 1. GPC remediation patch contract

### 1.1 Canonical owner（强制）

```text
Canonical identity state:
  artifact.policy.navigator.gpcPolicy ∈ { native, managed-opt-out }
  （引擎侧唯一投影键 MaskConfig["navigator.globalPrivacyControl"]:
    managed-opt-out ⇒ true；native ⇒ 键不存在）

Native Firefox prefs:
  privacy.globalprivacycontrol.enabled
  privacy.globalprivacycontrol.functionality_enabled
  —— 仅为 canonical policy 的 derived projection，不是第二状态源

Window / Worker / HTTP:
  全部消费同一 native pref machinery；不存在任何直读 MaskConfig 的伪装 getter
```

选定架构（最强形态）：**三投影全部走 native machinery，删除 Worker 专用 override。**
依据：FF152 中三者语义本就同构——

| 暴露面 | FF152 锚点 | 共享判定式 |
| --- | --- | --- |
| Window | `dom/base/Navigator.cpp` `Navigator::GlobalPrivacyControl()` | `functionality_enabled && (enabled \|\| (pbmode_enabled && 私 browsing))` |
| Worker | `dom/workers/WorkerNavigator.cpp` 同名 getter（原生体） | 完全相同（principal 判私 browsing） |
| HTTP | `netwerk/protocol/http/nsHttpChannel.cpp` `SetGlobalPrivacyControl()`（L11751–11760） | 完全相同 → `SetHeader("Sec-GPC","1")` |

因此只需在父进程把 canonical bool 投影进两个 pref，三个暴露面自动一致；
Worker override（fingerprint-injection.patch 引入的 `MaskConfig::GetBool` 直读）被回退，
不再存在可独立漂移的第二状态。

### 1.2 GPC policy-state 模型（Conditional Gate 修正，冻结）

数据模型**不是自由 bool**。GPC 的真实身份语义只有一个受管方向——opt-out 声明
（`true` = 用户明确表达禁止出售/分享；`Sec-GPC` 只有 opt-out 信号；`false` =
"没有表达"，不是可管理的网站可见状态）。因此：

```text
GPC policy states:  managed-opt-out | native
```

| Policy 状态（artifact.policy.navigator.gpcPolicy） | resolvedConfig 引擎键 | Pref 写入 | Window / Worker | Sec-GPC |
| --- | --- | --- | --- | --- |
| `managed-opt-out` | `navigator.globalPrivacyControl = true`（必在，值恰为 true） | `enabled=true` 且 `functionality_enabled=true` | `true` | `1` |
| `native` | 键**不存在**（必缺） | 无写入（prefs 保持 native 默认） | `false`（native） | 缺失 |

规则：

1. **显式 `false` 在 v4 中是非法形状**：validator 在生成与加载两侧均拒绝。
   "configured=false ⇒ 不管理 ⇒ 期望 native 恰好为 false" 的通道被移除，
   configured ≠ native-fallback 边界由此恢复——`false == missing` 的歧义以
   "false 不可表示" 的方式消解；
2. `pbmode_*` 两 pref 不在受管范围，永不写入；
3. 防御性运行时行为：若 MaskConfig 中出现布尔 false（正常流程不可达），投影点视同
   native、不写任何 pref；
4. 历史 v3 Artifact 不变：REQUIRED_CONFIG_KEYS 继续强制 BOOL（immutable），
   Gen5 A 的 `gpc=true` 即一次失败的 managed-opt-out 应用，判定不变；
5. v4 validator 双向规则：`gpcPolicy="managed-opt-out"` ⇔ 引擎键存在且为 true；
   `gpcPolicy="native"` ⇔ 键缺失。policy 字段位于 artifact.policy 命名空间，
   **不进入 resolvedConfig/CAMOU_CONFIG**（引擎只见 boolean 键本身，兼容
   properties.json contract）；
6. R1 验证不变量相应细化：managed-opt-out 下
   `Window == Worker == Sec-GPC present == true`；native 下无 managed 断言，
   观测仅作诊断记录，不参与失败判定。

可执行参考模型：`apps/camoufox-host/test_gpc_policy_contract.py`
（T3 的纯 Python 镜像，随本修正落地；后续 C++ seam 与 patch 作者化必须与之一致）。

### 1.3 Patch 系列（文件名与顺序）

新引擎修订使用独立下游系列目录（沿用 canvas 先例的 seam 校验机制）：

```text
diagnostic 候选修订: verisilo-camoufox-152.0.4-beta.28-r1-diag-v1
  apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/
    0000..0002  ← 现行 midl/canvas/bounded-close 三 patch 原样携带（digest 不变）
    0003-verisilo-gpc-canonical-pref-projection.patch
    0004-verisilo-remove-worker-gpc-mask-override.patch
    9000-verisilo-voices-diagnostics-DIAGNOSTIC-ONLY.patch

R1 正式候选修订: verisilo-camoufox-152.0.4-beta.28-r1-v1
  apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1/
    0000..0004  同上（无 9000）
    0005-verisilo-voices-final.patch   ← 仅在 V1–V4 收敛后另行起草审批
```

约束：

- `strict_build` 驱动拒绝在非 diag recipe 中包含带 `DIAGNOSTIC-ONLY` 标记的 patch
  （patch 头部第一行机器可读标记；driver 静态校验）；
- 诊断 logging **不得进入 R1 Engine Binding**，除非未来被明确提升为产品级 bounded
  runtime evidence 并重新过 Gate。

### 1.4 Seam 与验证步骤（沿用 canvas recipe 模式）

| Seam 文件 | Pre-image | Post-image 语义 |
| --- | --- | --- |
| 父进程初始化锚点（作者期定针：优先扩展现行 `browser-init.patch` 已落点的同一初始化路径；备选为 additions/camoucfg 新增投影单元被该路径调用） | 构建驱动 `verify-gpc-seam-preimage` | 单点、once、父进程守卫的 `Preferences::SetBool ×2` 投影块 |
| `dom/workers/WorkerNavigator.cpp` | `verify-worker-seam-preimage`（= fingerprint-injection 后状态） | 精确还原 FF152 release 原生 getter 体（golden 片段见附录 A），且全文件不再出现 `MaskConfig` GPC 引用 |

Driver 步骤顺序（新增，位于 upstream patches 之后、编译之前）：
`verify-gpc-seam-preimage → apply-0003 → verify-gpc-seam-postimage →
apply-0004 → verify-worker-native-restored →（diag recipe: apply-9000）`。

### 1.5 无浏览器回归（host 侧，随本合同落地）

- **T1 golden 还原断言**：0004 的文本包含附录 A 原生 getter 体逐行片段；
  系列应用后的补丁文本中 `navigator.globalPrivacyControl` 的 `MaskConfig::Get*`
  引用计数为 0。
- **T2 单写者断言**：整个系列中写 `privacy.globalprivacycontrol.*` 的位置恰为 1 处。
- **T3 投影策略模型测试**：`test_gpc_policy_contract.py` 纯 Python 镜像 §1.2
  policy-state 判定式——managed-opt-out/native 两态投影、非法形状（显式 false、
  policy 与引擎键不一致）拒绝、pbmode 组合不变性。
- **T4 driver 顺序测试**：recipe step 列表含 §1.4 五个新校验步且序正确。

---

## 2. Voices diagnostic patch contract（仅判别 V1–V4）

### 2.1 因果边界（冻结，永久措辞约束）

```text
observed:      cross-realm voice-inventory inconsistency（top 5 vs iframes 58）
causal axis:   realm-specific vs temporal-initialization —— UNRESOLVED
```

在判别实验收敛前，证据、文档、commit 信息一律禁用 "realm bug / realm-specific
defect" 作为因果结论；只允许引用上述两行。

### 2.2 Instrumentation 事件表

输出通道：stderr 单行 JSON，前缀 `VSIDIAG`，≤240 字符；
`uri` 一律 sha256 前 12 hex；时间戳为进程内单调毫秒；带 `proc:PARENT|CONTENT` 与 tid。

| 事件 | 注入点（文件 :: 函数） | 载荷 | 判别目标 |
| --- | --- | --- | --- |
| E1 | additions MaskConfig 消费侧：GetInstance 注入入口 | `{engaged, n}` | 配置形状/解析是否参与（V4 反证） |
| E2a/E2b | `windows/SapiService.cpp :: Init` 进/出（含线程标记） | `{}` / `{}` | SAPI 同步或异步（V2） |
| E3a/E3b | `nsSynthVoiceRegistry.cpp :: GetInstance` 受管批次进/出 | `{n}` | 注入完成时刻（V1/V2 时轴） |
| E4 | `SpeechSynthesisParent.cpp :: SendInit` | `{actorTag, n, invHash}` | 快照内容与时点（V1 核心） |
| E5 | `AddVoiceImpl` 广播处 | `{seq, uriHash}` | 增量投递时轴 |
| E6 | 内容侧 `RecvInitialVoicesAndState` / `RecvAddVoice` | `{n}` / `{uriHash}` | 镜像到达时刻 |
| E7 | `SpeechSynthesis.cpp :: GetVoices` 每 object 首次调用 | `{ctxId, n, t}` | 观测时刻 vs 投递时轴（V1/V2/V3） |
| E8 | `mVoiceCache` 插入/清空计数 | `{ins, clr}` | V3 发布/缓存假设 |

每事件有界；不打印语音名称原文；不含 profile/private 路径。

### 2.3 行为不变性硬约束（评审清单逐项打勾）

不得改变：语音顺序；初始化时序；native suppression 行为；受管清单内容。
不得引入：sleep/retry/yield；主动 `getVoices()` 触发；无界日志
（每进程事件总数上限 512，超出静默丢弃并计一个溢出事件）。
Patch 评审用静态禁词检查：`Sleep|Wait|delay|getVoices\(\)` 调用位为零。

### 2.4 判别决策表（冻结）

| 观测签名（VSIDIAG 时间轴） | 结论 |
| --- | --- |
| E4(n<受管数) 早于 E3b | **V1** 父进程懒初始化晚于首个绑定成立 |
| E2b 晚于 E3b（原生后到）或反之显著错峰 | **V2** 异步枚举交错成立 |
| E7 同一 object 二次查询计数变化而 E5/E6 其间无新投递 | **V3** 缓存/发布伪影成立 |
| E1 engaged=false 或 E3a 从未出现（配置已证明送达） | **V4** hunk 落点漂移嫌疑成立 → 转 source-level diff 审计 |

允许多签并存（如 V1+V2 复合）。实验收敛报告提交主脑后才起草 0005 final patch。

---

## 3. Enforcement control ≠ identity declaration（冻结模型）

```text
Identity declaration（网站可见身份态）:
  voices[] 清单本身
  fakeCompletion 对外行为（影响网站可观测的 SpeechSynthesis 事件时线）
    → 属 policy-controlled semantics，进入 identity policy/stableWebsiteFields

Engine enforcement control（执行语义，非身份属性）:
  native inventory suppression
    → 由受管清单存在性【派生】为 REQUIRED，不是自由开关
  voices:blockIfNotDefined
    → 允许进入 resolvedConfig，但只能是 policy 确定性派生的 engine config key，
      由 artifact 生成器在 voices 非空时恒定写入 true；
      不是用户可单独决定的 identity property
  voices:fakeCompletion:charsPerSecond
    → enforcement 参数，由 policy 派生
```

Validator 冻结规则（A-R1/B-R1 生成合同实现）：`voices` 非空 而 suppression 关闭
的 Artifact 形状为**违例**，生成期直接拒绝。

fakeCompletion 追加冻结（Conditional Gate）：

1. **不得成为独立随机 Artifact fingerprint dimension**：生成器禁止按 artifact
   随机化 `fakeCompletion` / `charsPerSecond`；
2. 二者由 `voicesMode` / engine policy 确定性派生（或由显式 policy variant 声明）；
3. 同一 voices identity 在不同 Artifact 中必须派生出相同的 completion 语义；
   `fakeCompletion` 不同的两个 Artifact 视为**不同的 identity contract**，
   不允许以同一身份名义比较。

## 4. FP1-R1 Carry-Forward Qualification（答案已冻结，不留 TBD）

新 R1 binary 不自动继承历史 FP1 Accepted；亦不重做历史 deBG source archaeology。
两层资格，全部通过后方可进入 FP2-R1：

**层一 · 静态 binding closure**

```text
canvas seam pre/post digest        与上一构建逐字节比对一致
bounded-close seam pre/post digest 同上
patch 应用顺序                      upstream 50 + 下游编号次序不变
新绑定完备                          新 source revision / archive SHA / executable SHA /
                                   tree manifest / BuildID / SourceStamp 全部落入新 lock
```

**层二 · Browser replay qualification**

使用全新 `A-R1 / B-R1` 在新 binary 上重证 deterministic replay（canvas export 确定性与
核心表面重放）。此步需要浏览器，属独立主脑授权，发生在 diag 判别与 0005 定稿之后。

## 5. Candidate lineage tree（正式历史结构）

```text
Candidate C0（retired）
└─ FP2 Gen1–Gen5 ………………… FAILED
Diagnostic remediation candidate(s)
└─ capability investigation only（9000 系列；永不承载产品 claim）
Candidate R1
├─ Policy verisilo.identity-policy.v4-dnt-native
├─ Artifact A-R1 / B-R1（新生成，validator 按 §3 规则）
├─ FP1-R1 Carry-Forward Qualification（§4）
└─ FP2-R1 fresh one-shot lineage（artifacts/camoufox-fp2-r1/）
```

Generation 编号永久退役。

## 6. Gate 序列与授权边界

```text
本合同 Accepted
  → patch 作者化（0003/0004/9000；seam digest 实算入新 lock）
  → diagnostic 构建（Linux 绑定路线）
  → bounded voices 判别实验（需主脑单独授权浏览器）
  → V1–V4 收敛报告 → 0005 起草审批
  → clean R1 build → FP1-R1 层一 → 层二（浏览器授权）
  → FP2-R1 执行合同（浏览器授权）
```

---

## 附录 A · Worker getter 目标态（FF152 release 原生体，0004 必须精确还原）

```cpp
bool WorkerNavigator::GlobalPrivacyControl() const {
  bool gpcStatus = StaticPrefs::privacy_globalprivacycontrol_enabled();
  if (!gpcStatus) {
    JSObject* jso = GetWrapper();
    if (const nsCOMPtr<nsIGlobalObject> global = xpc::NativeGlobal(jso)) {
      if (const nsCOMPtr<nsIPrincipal> principal = global->PrincipalOrNull()) {
        gpcStatus = principal->GetIsInPrivateBrowsing() &&
                    StaticPrefs::privacy_globalprivacycontrol_pbmode_enabled();
      }
    }
  }
  return StaticPrefs::privacy_globalprivacycontrol_functionality_enabled() &&
         gpcStatus;
}
```

来源：hg.mozilla.org releases/mozilla-release @ `FIREFOX_152_0_4_RELEASE`
`dom/workers/WorkerNavigator.cpp`（本次分析实取）。0004 同时移除
fingerprint-injection.patch 为该文件加入的 `#include "MaskConfig.hpp"` 中仅因 GPC
存在的部分——若该 include 仍被同文件其他 hunk（appVersion/platform/UA/
hardwareConcurrency override）需要则保留，以"文件内 MaskConfig GPC 引用清零"为准绳。

## 附录 B · Sec-GPC 发射语义锚点

`nsHttpChannel::SetGlobalPrivacyControl()`（FIREFOX_152_0_4_RELEASE L11751–11760）：
`functionality_enabled && (enabled || (pbmode_enabled && 私有浏览))` ⇒
`SetHeader(nsHttp::GlobalPrivacyControl, "1")`。与 Window/Worker 判定式同构，
证明 §1.1 单点投影足以驱动三暴露面。
