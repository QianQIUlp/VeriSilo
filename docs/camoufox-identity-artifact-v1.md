# VeriSilo M1.1 + M2-0 — Resolved Camoufox Identity Artifact (corrected)

Status: **M1.1 replay fix and M2-0 boundary hardening accepted on this host —
observations only, `verified: false` everywhere.**

## Gate summary

| Gate | 判定 |
| --- | --- |
| 有效 Artifact 的 Linux 重放正确性 | 通过 |
| 稳定性与身份分离 | 通过 |
| 严格不可信输入边界 | 通过（递归 schema、类型严格、嵌套未知字段拒绝） |
| 证据冻结到 Git | 通过（tracked fixtures + manifest + code revision） |
| 开始 standalone M2 Host（Linux） | 条件允许（Host v1 已实现并通过集成测试） |
| 接入 Tauri / EngineAdapter | 暂不允许 |

M0.1 保持“本机兼容性 Gate 通过”的收紧表述：`noCamoufoxWebdlAttemptObserved`
只证明 `camoufox.webdl` 未触发（`outboundNetworkFullyObserved: false`），
且原始报告位于 gitignored 目录——不是供应链或网络行为验证。

## 被证明的链路

```text
磁盘 Artifact（单次字节读取）
→ expectedArtifactFileSha256 + sidecar 校验
→ 递归严格 schema（type(x) is int/bool、嵌套未知字段拒绝）
→ browser binding（archive SHA、BuildID、SourceStamp、properties.json SHA）
→ 解压树 manifest 校验（689 文件 / 1,284,408,846 字节）
→ deepcopy config → configuredIdentityDigest
→ launch_options() 发送完全相同的 CAMOU_CONFIG（零变异门禁）
→ 网站每次看到稳定、可区分的 ObservedWebsiteSignals
→ ObservedWebsiteDigest（仅网站可见值；无 artifactId、无内部 seed、
  无 canvas、无 Artifact 提供的字体输入）
```

## Schema V2 与严格校验

- `verisilo-camoufox-identity-policy/v2`：stableWebsiteFields /
  sessionVariableFields / unavailableFields / canvasClassification /
  timezoneMode（fixed）/ requiredConfigKeys（47 键闭包）。
- `verisilo-camoufox-resolved-identity/v2`：resolvedConfig +
  browserBinding + stableSignalsDeclared + configuredIdentityDigest +
  canonicalDigest。
- `verisilo-camoufox-stable-signal-projection/v2`：ConfiguredIdentityDigest +
  ObservedWebsiteSignals + ObservedWebsiteDigest。

`identity_policy.validate_artifact_strict` 现在递归校验 policy、
browserBinding、generatorVersions、stableSignalsDeclared、exclusions 与
resolvedConfig 的每个嵌套对象和列表成员；整数要求 `type(x) is int`
（`bool` 被拒绝），布尔要求 `type(x) is bool`；任何嵌套对象出现未知字段即
拒绝；`.sha256` sidecar 必需；Artifact 文件字节只读取一次（无 TOCTOU）。
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

## 字体证据（修正探针输入循环）

- 探针使用**固定字体 universe**（25 个家族，硬编码并注入，A/B/C 输入完全
  相同），字体宽度差异来自 engine/seed 层，不再是 Artifact 提供的输入差异。
- 增加伪造字体 negative controls（必须全部 unavailable）。
- 增加**宿主字体 negative controls**（fc-list 中不在 Artifact 列表/固定
  universe 内的家族）。实测发现：本机 DejaVu Sans/Mono/Serif 与 Liberation
  Mono 在页面仍可见——**注入列表未完全遮蔽宿主字体**。该结果作为
  `hostFontMasking` 证据记录，不进入 ObservedWebsiteDigest，且字体隔离
  结论不成立（M2+/M2-W 处理）。

## Canvas 分类（不变）

raw 像素稳定但三个不同 seed 下完全相同（seed 噪声未通过本探针场景体现）；
export（toDataURL）跨重启不稳定。两者都不计入 ObservedWebsiteDigest。

## 测试结果（M2-0 最终）

### 稳定性：identity-a × 5 冷启动

Accepted run：**`run-1785823932-74f1f3b2`**
（report.sha256 `3c7b49aa…e1dd`）。

| Start | disk==sent | diff | 退出码 | exit 文件 | Profile fresh | ObservedWebsiteDigest |
| --- | --- | --- | --- | --- | --- | --- |
| 1–5 | true | 空 | 0 | 存在 | 是 | `sha256:1bfa0ca058347422f7a32c2d9c006c3cf0ba08f3d57d1d872e0a69fa6e0cd905` |

5/5 摘要一致；`artifactFileSha256EveryStart` 5 次相同（每次从磁盘重读的真实
raw SHA 证据）；`diskReloadedEveryStart` 由此派生而非硬编码。**通过。**

### 分离：identity-a/b/c

Accepted run：**`run-1785823958-6de874c7`**。

| Artifact | canonicalDigest | ObservedWebsiteDigest |
| --- | --- | --- |
| identity-a | `sha256:2d3ddbc3…d6dd` | `sha256:1bfa0ca0…cd905` |
| identity-b | `sha256:83614085…79fc` | `sha256:0ac0246f…9880` |
| identity-c | `sha256:1fb0f585…663e` | `sha256:a78ff339…4666` |

两两不同；退出码 0、exit 文件存在、Profile fresh、config 零变异全部满足。
**通过。**

### Tamper：四模式启动前拒绝

Accepted run：**`run-1785823971-e003201a`**。digest / missing-field /
type-error / policy-mismatch 全部拒绝；即使重算 canonicalDigest 与 sidecar，
严格校验仍会因类型或一致性错误拒绝（单元测试覆盖）。

### 解压树

`browser-tree-manifest.json`（689 文件、1,284,408,846 字节，manifest SHA
`20807dd2…bae75`）每次运行前验证；缺失/多余/修改文件均在启动前拒绝。

## 单元与集成测试

- `test_identity_artifact.py`：14/14 通过（含 bool-as-int 拒绝、fonts 数字
  拒绝、7 类嵌套未知字段拒绝、expected file SHA 不匹配拒绝、sidecar 必需、
  重算摘要仍被拒、browser binding 核对、字体 universe 与 probe 同步）。
- `test_host_v1.py`：7/7 通过（hello 版本绑定、launch/status/close、
  Host 重启持久化、三次冷启动摘要一致、profile_in_use、SHA/schema/树篡改
  拒绝、浏览器崩溃 → failed → 锁释放 → 可重启动；stdout 全程纯协议）。

## M2 gate

**允许进入 M2 standalone Python Host（Linux）**：Host v1 已实现并通过上述
集成测试，详见 `docs/camoufox-host-v1.md`。M2-W（Windows 实机）、M3
（EngineAdapter/Tauri）仍不允许；Vault、代理、UI 顺序不变。

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
含 run-id、报告摘要、raw file SHA、browserBinding、解压树摘要与代码
Git revision）。
