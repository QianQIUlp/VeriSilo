# VeriSilo M1.1 + M2-0 + M2.0.1 + M2.0.2 + M2.0.3 — Resolved Camoufox Identity Artifact

Status: **M2.0.3 accepted on this host — observations only,
`verified: false` everywhere. Host v1 is a runnable Linux prototype; the
long-term schema/version contract is now frozen and old v2 artifacts are
explicitly rejected.**

## Gate summary

| Gate | 判定 |
| --- | --- |
| 有效 Artifact 的 Linux 重放正确性 | 通过（v3 Artifact / v2 ObservedWebsiteDigest） |
| 稳定性与身份分离 | 通过（inherit fontMode，字体宽度不入摘要） |
| 递归严格 Artifact 边界 | 通过（嵌套必需字段 + 未知字段 + 顶层 scalar 闭包） |
| Schema/摘要版本冻结 | **通过（M2.0.2：v3/v2 升级 + 旧版显式拒绝）** |
| Host 重启持久化 | 通过（固定 origin + Cookie API/页面/cookies.sqlite 三类证据） |
| 正常生命周期与进程树清理 | 通过 |
| 异常进程无法退出时 fail-closed | **通过（M2.0.3：全树身份确认 + 三态原子 quarantine）** |
| 严格 Artifact JSON / RFC3339 边界 | **通过（M2.0.3）** |
| 证据冻结到 Git | 通过（tracked fixtures + manifest + code revision） |
| M2-W Windows 手工 Gate | **允许（下一项）** |
| 接入 Tauri / EngineAdapter | 暂不允许 |

M0.1 保持“本机兼容性 Gate 通过”的收紧表述：`noCamoufoxWebdlAttemptObserved`
只证明 `camoufox.webdl` 未触发（`outboundNetworkFullyObserved: false`），
且原始报告位于 gitignored 目录——不是供应链或网络行为验证。

## 版本契约（M2.0.2）

- Artifact：`verisilo-camoufox-resolved-identity/v3`。
- Policy：`verisilo-camoufox-identity-policy/v3`（含必需 `fontMode`）。
- Projection：`verisilo-camoufox-stable-signal-projection/v3`。
- ObservedWebsiteDigest：`verisilo-camoufox-observed-website/v2`。
- M1 run report：`verisilo-camoufox-m1-run-report/v3`。
- 旧 `…/v2` Artifact 在 schema 检查阶段返回
  `UnsupportedSchemaVersionError`（协议码 `unsupported_schema_version`），
  **不会**被当作普通 missing-field 处理；未知/未来版本同样显式拒绝。

## 被证明的链路

```text
磁盘 Artifact（单次字节读取）
→ schema 版本契约检查（旧版显式拒绝）
→ expectedArtifactFileSha256 + sidecar 校验
→ 统一顶层 closed schema（含 47 键 resolvedConfig 闭包；
  嵌套必需字段、未知字段、类型全部递归校验）
→ generatedBy 非空字符串；generatedAtUtc 严格 RFC 3339 UTC（规范化为 Z）
→ browser binding（archive SHA、BuildID、SourceStamp、properties.json SHA）
→ 解压树 manifest 校验（689 文件 / 1,284,408,846 字节；
  symlink / 非 regular 文件同样拒绝）
→ deepcopy config → configuredIdentityDigest
→ launch_options() 发送完全相同的 CAMOU_CONFIG（零变异门禁）
→ 网站每次看到稳定、可区分的 ObservedWebsiteSignals
→ ObservedWebsiteDigest v2（无 artifactId、无内部 seed、无 canvas、
  inherit 模式下无字体宽度）
```

## 严格校验（M2.0.2）

- 顶层使用**统一 closed schema**：schema、artifactId、policy、
  browserRelease、browserBinding、generatedBy、generatedAtUtc、
  generatorVersions、resolvedConfig、stableSignalsDeclared、exclusions、
  configuredIdentityDigest、canonicalDigest 全部要求存在且类型正确。
- `generatedBy`：非空字符串。
- `generatedAtUtc`：严格 RFC 3339 UTC，末尾必须为 `Z`（`+00:00`、
  非 UTC 偏移、非 RFC3339 字符串均拒绝）。
- RFC3339 校验使用**显式正则 + 日期解析**：只接受
  `YYYY-MM-DDTHH:MM:SS[.fraction]Z`；空格分隔、basic form、缺秒、非 UTC
  表示全部拒绝。
- `browserRelease`：非空字符串。
- `configuredIdentityDigest` / `canonicalDigest`：`sha256:<64 hex>`。
- Artifact JSON 使用**严格解析器**：递归拒绝重复 key（嵌套对象同样拒绝）、
  拒绝 NaN/Infinity、顶层必须是 object；违规统一返回
  `integrity_rejected`，不会表现为 `internal_error`，也不会因解析器选择
  不同重复值而产生歧义（为未来 Rust/Python 共享格式铺路）。
- `type(x) is int` / `bool` 被拒绝；所有嵌套对象拒绝未知字段且要求
  必需字段齐全；`.sha256` sidecar 必需；Artifact 字节只读取一次（无
  TOCTOU）。

## 双摘要规则

规范 JSON：UTF-8、递归排序键、紧凑分隔符、`ensure_ascii=false`、
`allow_nan=false`。

- Artifact canonicalDigest：去掉 `canonicalDigest` 后的规范 JSON 摘要。
- ConfiguredIdentityDigest（v1 语义不变）：resolvedConfig 的规范摘要
  （可含 seed，不含 artifactId）。
- ObservedWebsiteDigest（v2）：仅网站可见值（不含 artifactId、内部 seed、
  canvas、Artifact 字体输入；inherit 模式不含字体宽度）。

## 字体策略

`policy.fontMode` 二分为 `inherit` / `managed`：

- `inherit`（当前三个 fixture）：`fontUniverseWidths` 是 **host-bound**
  证据，不进入 ObservedWebsiteDigest；`fontNegativeControls`（伪造字体）
  保留。
- `managed`：只有全部宿主 negative controls 在页面中 unavailable 时，
  字体宽度才进入摘要；否则浏览器会被**临时启动并探测**，随后清理，
  launch 在进入 `running` 之前失败（`host_font_masking_failed`）。这
  **不是**“浏览器启动前失败”——检测必须在真实页面中完成。

当前宿主字体遮蔽仍未解决，因此：同一 Linux 主机上摘要可稳定；跨主机
字体相关摘要未必稳定；字体隔离**不宣称**。

## 进程身份与 quarantine（M2.0.2）

- `exit_supervisor` 写入 supervisor PID、child PID、start-time ticks、
  process group；Host 据此建立受管进程身份，不再用 `/proc` cmdline 猜 PID。
- 发信号前核对 **PID + start-time**：PID 被复用但 start-time 不同时，
  不会被当作原进程，也不会被误杀。
- 正常 close/fail：确认整个受管进程树退出后才释放 profile lock。
- 无法确认退出（`processTreeExit.exited !== true`）：状态为
  **quarantined**，当前 Host **继续持有 profile lock**，写入
  `stateRoot/quarantine/<profile>.json`（含 PID、start-time、process
  group），绝不标记为 exited。
- M2.0.3：`exited=true` 只在**所有捕获的 PID+start-time identity（根与
  后代）**都消失后返回；存活后代计入 `remaining` 并写入 quarantine。
- quarantine 记录采用**三态读取**（absent / valid / invalid）：文件存在但
  截断、缺字段、类型错误或不可读时**阻止接管**；写入为原子替换
  （同目录临时文件 + fsync + `os.replace` + 目录 fsync），写入/删除失败
  fail-closed。
- 新 Host 只有验证 quarantine 记录中所有 PID+start-time 身份已不存在后
  才能清理记录并接管；否则返回 `profile_quarantined`。

## Canvas 分类（不变）

raw 像素稳定但 seed 噪声未通过本探针场景体现；export（toDataURL）跨重启
不稳定。两者都不计入 ObservedWebsiteDigest。

## 测试结果（M2.0.2 最终）

### 稳定性：identity-a × 5 冷启动

Accepted run：**`run-1786158540-228a3340`**
（report.sha256 `b15625fd…b323d`）。

| Start | disk==sent | diff | 退出码 | exit 文件 | Profile fresh | ObservedWebsiteDigest v2 |
| --- | --- | --- | --- | --- | --- | --- |
| 1–5 | true | 空 | 0 | 存在 | 是 | `sha256:70f71b5ca7ee287a4d5e989086a990264433129615f0ee988aa18c135385d0ff` |

5/5 摘要一致；`artifactFileSha256EveryStart` 5 次相同；`fontModeEveryStart`
全为 `inherit`。**通过。**

### 分离：identity-a/b/c

Accepted run：**`run-1786158560-43ea2bd1`**。

| Artifact | canonicalDigest | ObservedWebsiteDigest v2 |
| --- | --- | --- |
| identity-a | `sha256:4f602179…58e8` | `sha256:70f71b5c…d0ff` |
| identity-b | `sha256:0cb8e396…42f7` | `sha256:8559d3e0…6324` |
| identity-c | `sha256:d6b8ad04…713f` | `sha256:f873b3dd…6de13` |

两两不同；退出码 0、exit 文件存在、Profile fresh、config 零变异全部满足。
**通过。**

### Tamper：四模式启动前拒绝

Accepted run：**`run-1786158573-fef25c08`**。digest / missing-field /
type-error / policy-mismatch 全部拒绝；Host 集成测试额外验证删除
`policy.canonicalJsonRule`（重算摘要与 sidecar）在启动前
`integrity_rejected`。

### 解压树

`browser-tree-manifest.json`（689 文件、1,284,408,846 字节，manifest SHA
`20807dd2…bae75`）每次运行前验证；缺失/多余/修改文件、symlink、FIFO 等
非 regular 条目全部拒绝。

## 单元与集成测试

- `test_identity_artifact.py`：21/21 通过（新增：旧 v2 Artifact →
  `UnsupportedSchemaVersionError`；`generatedBy` 非字符串拒绝；
  `generatedAtUtc` bool / 非 RFC3339 / 非 UTC / 非 Z 规范化拒绝；
  `browserRelease` 与 digest 格式拒绝；RFC3339 宽松形式拒绝；严格 JSON
  解析器拒绝重复 key / NaN / 非 object）。
- `test_host_v1.py`：19/19 通过（新增：协议级 v2 → `unsupported_schema_version`；
  managed 字体失败后浏览器清理且状态从未 `running`；quarantine 阻止接管
  直到原进程身份消失；PID 相同但 start-time 不同不被视为原进程；
  quarantine 保留 profile lock；根退出 + 忽略 SIGTERM 后代存活的反例；
  损坏 quarantine 文件阻止接管；原子写入/失败 fail-closed；非 object /
  重复 key / NaN Artifact 协议级 `integrity_rejected`）。

## M2 gate

**允许进入 M2-W（Windows 手工 Gate）**：Linux standalone Host v1 作为
“可运行的 Linux 原型”已通过 M2.0.2 修正与验收（版本契约冻结、顶层类型
闭包、进程身份/start-time 校验、quarantine fail-closed）。M3
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
含 run-id、报告摘要、raw file SHA、browserBinding、解压树摘要、schema
版本契约与代码 Git revision）。
