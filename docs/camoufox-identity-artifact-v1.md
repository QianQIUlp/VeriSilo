# VeriSilo M1.1 + M2-0 + M2.0.1 — Resolved Camoufox Identity Artifact (corrected)

Status: **M2.0.1 host-correctness hardening accepted on this host —
observations only, `verified: false` everywhere. Host v1 is a runnable Linux
prototype, not a fully accepted product Host.**

## Gate summary

| Gate | 判定 |
| --- | --- |
| 有效 Artifact 的 Linux 重放正确性 | 通过 |
| 稳定性与身份分离 | 通过（inherit fontMode，字体宽度不入摘要） |
| 严格不可信输入边界 | **通过（M2.0.1：嵌套必需字段完整性 + 递归校验）** |
| Host 重启持久化 | **通过（M2.0.1：固定 probe origin + Cookie/API/页面/cookies.sqlite 三类证据）** |
| Host 生命周期（SIGTERM/SIGINT/EOF、进程树退出确认、锁最后释放） | 通过（Linux 原型） |
| 证据冻结到 Git | 通过（tracked fixtures + manifest + code revision） |
| 开始 M2-W Windows 手工 Gate | 暂缓（下一项） |
| 接入 Tauri / EngineAdapter | 暂不允许 |

M0.1 保持“本机兼容性 Gate 通过”的收紧表述：`noCamoufoxWebdlAttemptObserved`
只证明 `camoufox.webdl` 未触发（`outboundNetworkFullyObserved: false`），
且原始报告位于 gitignored 目录——不是供应链或网络行为验证。

## 被证明的链路

```text
磁盘 Artifact（单次字节读取）
→ expectedArtifactFileSha256 + sidecar 校验
→ 递归严格 schema（type(x) is int/bool、嵌套未知字段拒绝、
  嵌套必需字段缺失拒绝）
→ browser binding（archive SHA、BuildID、SourceStamp、properties.json SHA）
→ 解压树 manifest 校验（689 文件 / 1,284,408,846 字节；
  symlink / 非 regular 文件同样拒绝）
→ deepcopy config → configuredIdentityDigest
→ launch_options() 发送完全相同的 CAMOU_CONFIG（零变异门禁）
→ 网站每次看到稳定、可区分的 ObservedWebsiteSignals
→ ObservedWebsiteDigest（仅网站可见值；无 artifactId、无内部 seed、
  无 canvas、inherit 模式下无字体宽度）
```

## Schema V2 与严格校验

- `verisilo-camoufox-identity-policy/v2`：stableWebsiteFields /
  sessionVariableFields / unavailableFields / canvasClassification /
  timezoneMode（fixed）/ fontMode（inherit | managed）/
  requiredConfigKeys（47 键闭包）。
- `verisilo-camoufox-resolved-identity/v2`：resolvedConfig +
  browserBinding + stableSignalsDeclared + configuredIdentityDigest +
  canonicalDigest。
- `verisilo-camoufox-stable-signal-projection/v2`：ConfiguredIdentityDigest +
  ObservedWebsiteSignals + ObservedWebsiteDigest。

`identity_policy.validate_artifact_strict` 递归校验 policy、
browserBinding、generatorVersions、stableSignalsDeclared、exclusions 与
resolvedConfig 的每个嵌套对象和列表成员；整数要求 `type(x) is int`
（`bool` 被拒绝），布尔要求 `type(x) is bool`。**M2.0.1 修正：closed
object 不仅拒绝未知字段，还要求 schema 声明的每个字段都存在**
（policy.canonicalJsonRule、exclusions.tokens、WebGL contextAttributes、
voice 字段、declared screen 字段、binding/generator 字段等缺失均在启动前
拒绝）；`.sha256` sidecar 必需；Artifact 文件字节只读取一次（无 TOCTOU）。
`verify_artifact_raw(path, expected_file_sha)` 让 launch 请求携带
`expectedArtifactFileSha256` 成为强制校验。

## 双摘要规则

规范 JSON：UTF-8、递归排序键、紧凑分隔符、`ensure_ascii=false`、
`allow_nan=false`。

- Artifact canonicalDigest：去掉 `canonicalDigest` 后的规范 JSON 摘要。
- ConfiguredIdentityDigest：resolvedConfig 的规范摘要（可含 seed，
  不含 artifactId）。
- ObservedWebsiteDigest：仅网站可见值（不含 artifactId、内部 seed、canvas、
  Artifact 字体输入）。

## 字体策略（M2.0.1）

`policy.fontMode` 二分为 `inherit` / `managed`：

- `inherit`（当前三个 fixture）：`fontUniverseWidths` 是 **host-bound**
  证据，**不进入 ObservedWebsiteDigest**（宿主字体泄漏不能通过字体宽度
  间接进入跨主机稳定摘要）；`fontNegativeControls`（伪造字体）保留。
- `managed`：只有全部宿主 negative controls 在页面中 unavailable 时，
  字体宽度才进入摘要；否则 Host/Spike 在启动前以
  `host_font_masking_failed` 拒绝。

当前宿主字体遮蔽仍未解决（`document.fonts.check` 仍能看到多个宿主字体
族），因此：同一 Linux 主机上摘要可稳定；跨主机字体相关摘要未必稳定；
字体隔离**不宣称**；只有 `managed` 且负向控制全部通过后才纳入 managed
identity Gate。

## Canvas 分类（不变）

raw 像素稳定但三个不同 seed 下完全相同（seed 噪声未通过本探针场景体现）；
export（toDataURL）跨重启不稳定。两者都不计入 ObservedWebsiteDigest。

## 测试结果（M2.0.1 最终）

### 稳定性：identity-a × 5 冷启动

Accepted run：**`run-1785827166-a7885889`**
（report.sha256 `821cb415…c53f1`）。

| Start | disk==sent | diff | 退出码 | exit 文件 | Profile fresh | ObservedWebsiteDigest |
| --- | --- | --- | --- | --- | --- | --- |
| 1–5 | true | 空 | 0 | 存在 | 是 | `sha256:6206f58db1a9e20cd6681db783d9dbdefd56d4cc7ab02da322ed09167e296f03` |

5/5 摘要一致；`artifactFileSha256EveryStart` 5 次相同（每次从磁盘重读的真实
raw SHA 证据）；`fontModeEveryStart` 全为 `inherit`。**通过。**

### 分离：identity-a/b/c

Accepted run：**`run-1785827191-786919a8`**。

| Artifact | canonicalDigest | ObservedWebsiteDigest |
| --- | --- | --- |
| identity-a | `sha256:83698aea…812e` | `sha256:6206f58d…96f03` |
| identity-b | `sha256:bfd11ea9…96f7` | `sha256:414b8ee8…fdbf` |
| identity-c | `sha256:28bab21f…66a1` | `sha256:e6077b79…96df4` |

两两不同；退出码 0、exit 文件存在、Profile fresh、config 零变异全部满足。
**通过。**

### Tamper：四模式启动前拒绝

Accepted run：**`run-1785827209-90d65d94`**。digest / missing-field /
type-error / policy-mismatch 全部拒绝；即使重算 canonicalDigest 与 sidecar，
严格校验仍会因类型或一致性错误拒绝（单元测试覆盖）。Host 集成测试额外
验证：删除 `policy.canonicalJsonRule`（重算全部摘要与 sidecar 后）在启动前
被 `integrity_rejected`。

### 解压树

`browser-tree-manifest.json`（689 文件、1,284,408,846 字节，manifest SHA
`20807dd2…bae75`）每次运行前验证；缺失/多余/修改文件均在启动前拒绝，
**symlink 与 FIFO 等非 regular 条目同样拒绝**（单元测试覆盖）。

## 单元与集成测试

- `test_identity_artifact.py`：17/17 通过（新增 9 类嵌套必需字段缺失拒绝、
  fontMode fixture 检查、树 symlink/FIFO 拒绝）。
- `test_host_v1.py`：11/11 通过（hello 版本绑定、launch/status/close、
  **真实跨进程持久化**（固定 origin，bootCount 0→1→2，Cookie API/页面/
  cookies.sqlite 三类证据）、三次冷启动摘要一致、profile_in_use、
  SHA/schema/嵌套缺失字段/树篡改拒绝、浏览器崩溃 → failed → 锁释放 →
  可重启动、**SIGTERM/SIGINT/stdin EOF 活跃会话清理**、超长帧内存有界拒绝；
  stdout 全程纯协议）。

## M2 gate

**允许继续 M2-W（Windows 手工 Gate）**：Linux standalone Host v1 作为
“可运行的 Linux 原型”已通过上述修正与验收；它不是已完成验收的产品 Host
（字体隔离未解决、Windows 生命周期未实现）。M3（EngineAdapter/Tauri）
仍不允许；Vault、代理、UI 顺序不变。

## 复现

```bash
cd apps/camoufox-host
uv sync --frozen --offline
uv run python test_identity_artifact.py
uv run python test_host_v1.py

uv run python run_identity_spike.py stability --artifact ../../tests/fixtures/camoufox/identity-a.json --runs 5
uv run python run_identity_spike.py separation --artifacts ../../tests/fixtures/camoufox/identity-a.json,../../tests/fixtures/camoufox/identity-b.json,../../tests/fixtures/camoufox/identity-c.json
uv run python run_identity_spike.py tamper --artifact ../../tests/fixtures/camoufox/identity-a.json --out-dir ../../artifacts/camoufox-m1/tampered
```

证据索引：`tests/fixtures/camoufox/evidence-manifest.json`（tracked，
含 run-id、报告摘要、raw file SHA、browserBinding、解压树摘要、字体策略与
代码 Git revision）。
