# R1-diag Durable Builder Evidence & Rehydration Contract

> **历史 builder 合同**：diagnostic build/provenance 已关闭；本文不再是当前执行约束。
> 仅在审计对应 builder evidence 时读取，当前路线见
> [状态页](camoufox-program-status.md)。

- 状态：**Accepted for implementation**
- 形成日期：2026-08-23
- 起始 checkpoint：`f267bb4ff3f00115a37546bbe0649d0db889a7d3`
- 起始 tree：`d923535f105169b40d3ae091b9b925c864a086d7`
- 执行边界：offline / no-engine / no-browser

本文冻结 Phase B builder identity 的 durable retention 与 Docker image
rehydration 合同。它不改变 R1 diagnostic engine recipe、patch series 或运行时语义，
也不授权 Phase B-2、Phase C-2、Phase D、Browser 或 V1–V4。

## 1. 历史与当前状态

`r1diag-builder-20260823t0504z` 曾合法产生 image
`sha256:f46ec076dcde9b3759007c3683c07e5a3c563f9145475b335b6f40a82bb6732c`，
并在 `f267bb4` 被正确写入 v2 source lock。宿主 deallocate/reboot 后，Azure
Temporary Storage 被 cloud-init 重新格式化，Phase-B material evidence 与 Docker
content store 同时永久丢失。因此：

```text
Phase B-1 / Phase C-1 identity history: Accepted and immutable
Phase C-1 operational consumability:   Superseded / unavailable
Current builder binding:                must return to unbound
```

`f267bb4` 不删除、不改写。新的 checkpoint 通过 Git 历史和 lock 内的 operational
lineage 指针保留该事实，不把旧 result、archive 或 image 冒充为当前可消费材料。
历史成功 run `r1diag-builder-20260823t0504z` 与更早失败 run
`r1diag-builder-20260823t0435z` 均永久 retired；未来 Phase B-2 即使位于全新 scratch/
durable filesystem 也必须拒绝复用这两个 run-id。

## 2. v2 compatibility decision

当前 Dockerfile、embedded strict driver 与 diagnostic gate 均固定读取：

```text
camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json
schema verisilo-r1-diag-source-binding/v2
engineRevision verisilo-camoufox-152.0.4-beta.28-r1-diag-v2
```

本任务禁止修改这些 container recipe bytes。因此不新建 strict driver 无法消费的 v3
execution schema。新的 Phase-A checkpoint 更新现行 v2 lock 的 host-evidence contract，
并保持 Dockerfile、strict driver、diag gate、engine revision 与六个 patch bytes 不变。
历史 v2 binding 由 `f267bb4` 及 lock 内 lineage 字段保存。

## 3. Scratch 与 durable evidence 分层

```text
scratch root:
  /mnt/camoufox-build
  可用于可重建的 source/build/work/output
  允许宿主重启后丢失

durable evidence root:
  /var/lib/verisilo/camoufox-build-evidence
  只保存 builder evidence bundle 与 durable-root qualification
  必须跨宿主重启保持
```

两个路径均由 launcher 常量冻结；不接受环境变量、任意 lock path、任意 evidence path
或 CLI root override。durable root 必须由宿主预先创建并允许 launcher 写入；launcher
不得执行 `chmod`、`chown` 或 privileged filesystem repair。

每次使用 durable root 前必须 fail closed 验证：

1. root 是真实目录且不是 symlink；
2. root 与 scratch root 的 `st_dev` 不同；
3. 记录 `findmnt` 的 mount target/source、filesystem type 与 UUID；
4. 当前 mount identity 与 reboot qualification 完全一致；
5. launcher 可直接创建、fsync、重读证据，不借助 `sudo` 修复文件权限。

## 4. Reboot qualification

Phase B-2 前必须完成一次 versioned qualification：

```text
stage-durable-root-qualification
  → exclusive qualification directory
  → sentinel + request JSON
  → fsync file and directory

宿主 reboot/deallocate/restart

verify-durable-root-qualification
  → boot ID 必须不同
  → mount identity 必须相同
  → sentinel size/SHA 必须相同
  → 写入 exclusive qualified-after-reboot result
  → fsync + re-read
```

qualification ID 只能选择固定 root 下的一个直接子目录，不能成为路径注入通道。
Phase B `prepare-image` 必须显式绑定一个已通过的 qualification ID；缺失、同 boot、
mount drift、sentinel drift 或 result drift 均在 Docker build 前拒绝。

## 5. Durable Phase-B bundle

每个新 Phase-B run 对应：

```text
/var/lib/verisilo/camoufox-build-evidence/<run-id>/
  retention-preflight.json
  builder-image.tar
  builder-image-result.json
  builder-image-inspect.json
  builder-build-context.tar
  buildx.log
  buildx-metadata.json
  docker-save.log
  durable-manifest.json
  retention-receipt.json
```

在任何 Docker build 前，launcher 必须以新 run-id exclusive 创建 bundle directory，
写入并 fsync/re-read `retention-preflight.json`，绑定 qualification result 与当前 mount
identity，并证明 launcher 当前仍能直接 create/write/fsync/read。reservation 失败或目录
已存在时不得进入 Docker；成功后的 reservation 即使后续失败也永久保留，禁止同 run-id
重试。

其余七个 payload 文件必须从 scratch provenance 以 exclusive create 复制；每个目标文件由
launcher 写入、flush、`fsync`、关闭后重新 hash/stat。bundle directory pre-exists
（除本次 launcher 刚创建并验证的 exact preflight reservation）即拒绝，不覆盖、不删除、
不在同一 run-id 重试。复制失败可保留 partial bundle，
但不得生成 accepted manifest。

`durable-manifest.json` 在七个 payload 之后写入，至少包含：

- schema、runId、writtenAt、`retained=true`、`fsyncCompleted=true`；
- source commit/tree/lock、imageId、binding proposal canonical SHA；
- durable qualification ID/result SHA 与 mount identity；
- preflight 与其余七个 payload 文件的精确 basename、size 和 SHA-256；
- manifest canonical SHA-256。

manifest canonical hash 的输入定义为“移除
`manifestCanonicalSha256` 字段后的 compact/sorted-key UTF-8 JSON object”；manifest
本身不列入 files，避免自引用。manifest 写入并 fsync 后，launcher 必须从 durable
root 重新打开并完整验证 payload、manifest、qualification 和 image identity。只有该
重读通过，才写入最终 exclusive+fsync `retention-receipt.json`；receipt 至少绑定
manifest raw/canonical SHA、builder result SHA、proposal canonical SHA、qualification
result SHA、source lineage，并声明 `retained=true / reReadable=true`。receipt 自身采用
移除 `receiptCanonicalSha256` 后的同一 canonical JSON 规则，写入后还须再次完整重读。

manifest 是 payload commit marker；receipt 才是“payload 已从 durable root 重读闭合”的
transaction marker。manifest 已存在但 receipt 缺失、漂移或不可重读时，Phase B/Phase C
都必须拒绝，避免把“准备重读”误报为“已经重读”。

`builder-image-result.json` 在 retention 前生成，状态为
`prepared-awaiting-durable-retention`。它不单独代表 accepted Phase B；
`retention-receipt.json` 的
`durably-retained-reread-verified-awaiting-source-lock-binding` 状态才是 Phase C-2
可接受的 transaction boundary。

Phase B 从 buildx metadata 得到 immutable config digest 并完成 exact inspect 后，
`docker image save` 必须按该 ID
保存，不能继续按 mutable tag 保存。launcher 还必须解析 Docker archive 的
`manifest.json` 与 config member，验证 config bytes 的 SHA-256 恰为 proposal image
ID；仅凭 save command 成功不能证明 tar 与已 inspect identity 相同。

BuildKit 不得在 recipe 校验后继续读取可变 live checkout directory。launcher 必须把
Dockerfile、strict driver、diag gate 与 host launcher 的受锁 bytes 冻结为 deterministic
`builder-build-context.tar`，以同一个 `O_NOFOLLOW` FD 做 pre-hash、buildx binary stdin
和 post-hash；该 tar 进入 durable bundle，并由 Phase C evidence 绑定 SHA/size。buildx
完成后，image identity 取自 metadata 的 immutable `containerimage.config.digest`，随后按
该 digest inspect；inspect ID 与四个 frozen recipe-source labels 必须全部一致。mutable
tag 只用于本地 `--load` 命名，不得成为 inspect/save/binding authority。

## 6. Bound evidence 与 rehydration

Phase C-2 将来必须同时冻结：

- exact `builderImageBinding`；
- builder result SHA；
- binding proposal canonical SHA；
- durable manifest raw/canonical SHA；
- retention receipt raw/canonical SHA 与 `reReadable=true`；
- deterministic build-context tar SHA/size；
- durable qualification ID/result SHA；
- `retained=true` 与 source commit/tree/lock lineage。
- `builderOperationalLineage.current` 必须从 exact unbound state 原子推进为
  `bindingState=bound / durableEvidence=retained-and-reread / phaseB2=accepted /
  reasonCodes=[]`；不得留下“顶层已 bound、lineage 仍 unbound”的矛盾。

`prepare-bound-image` 不再消费任意 `--source-run-root`。它只按 source run ID 从固定
durable root 读取 bundle，先完整验证 manifest/files/result/binding，再检查 Docker store：

```text
bound image ID 已存在且 inspect ID 精确相等
  → action = already-present

bound image ID 不存在
  → docker image load（verified tar 作为 binary stdin）
  → load exit 必须为 0
  → 重新 inspect immutable image ID
  → loaded ID 必须等于 lock binding
```

rehydration 只反序列化已冻结 image archive，不是新 build。禁止 network pull、
`docker build`、tag authority、shell redirection、`--entrypoint`、driver injection 或
手工 filesystem repair。load 输出不构成 identity；load 后的 immutable inspect ID
才构成 Gate。

load 路径必须以 `O_NOFOLLOW`（平台支持时）打开一个 regular-file archive FD；pre-load
hash/fstat、Docker stdin 与 post-load hash/fstat 使用同一个已打开 FD，避免“校验一个
path、加载另一个 bytes”的 TOCTOU。任何 inode/size/hash 漂移均拒绝。

`build-engine` 仅接受状态为 `prepared-from-durable-builder-binding`、
`retained=true` 且 proposal/manifest 与 bound lock 精确相等的 preparation record；
该 record 不是独立 authority：engine path 必须按其 source run ID 再次从固定 durable root
重读 manifest、receipt、全部 payload 并与 bound lock 精确比较，然后才在 `docker run`
前再次 inspect exact image ID。若 image 在 prepare 与 build 之间丢失，
build fail closed，不在 build path 隐式 load/pull/build。
`docker run` 必须显式携带 `--pull=never`，不能依赖 Docker 对 image-ID reference 的
隐式解析行为。

所有 Docker 调用使用冻结的绝对 executable path 与最小净化环境；
`DOCKER_HOST`、`DOCKER_CONTEXT`、proxy、任意 `PATH` 或继承环境不得改变 daemon、
network 或命令解析。证据 JSON 必须拒绝 duplicate keys；exclusive JSON write 必须
使用原子 exclusive-create 语义，不能采用 `exists()` 后普通覆盖写的 TOCTOU 模式。

## 7. 必须覆盖的 no-browser regressions

至少覆盖：

1. durable root 与 scratch 同 filesystem、symlink 或 mount drift 拒绝；
2. qualification 同 boot、sentinel drift、result drift 拒绝；
3. Docker 前 durable reservation/preflight 失败时不启动 build；ephemeral success 但
   durable copy/fsync/re-read/receipt 任一步失败时 Phase B 失败；
4. manifest 缺文件、未知文件、path traversal、hash/size drift 拒绝；
5. bundle pre-exists 不覆盖；partial bundle、仅有 manifest 但无 receipt 都不可接受；
6. copy/fsync 后从 durable root 重新读取；模拟 reboot 后仍可验证；
7. image absent + valid tar 进入 load；already present + exact ID 不 load；
8. tar drift 在 load 前拒绝；load nonzero 或 loaded ID mismatch 拒绝；
9. rehydration 路径无 chmod/chown、pull、build、tag trust；
10. Phase C/Phase D 只消费 `retained=true / reReadable=true` 且
    manifest+receipt-bound evidence；
11. 历史 `f46ec076…`/run 只存在于 superseded lineage，不成为当前 binding；
12. Dockerfile、strict driver、diag gate 与 0000–0004/9000 bytes 保持不变。
13. inspect 后 tag 被重定向、tar config digest 不等于 image ID、JSON duplicate key
    与 hostile Docker environment 全部拒绝。
14. 两个历史 Phase-B run-id 永久拒绝复用；bound/unbound lock 与 operational lineage
    current 状态不一致时拒绝。

## 8. 允许、禁止与停止条件

允许修改：

```text
apps/camoufox-host/build/r1-diag-v1/build_host.py
apps/camoufox-host/test_r1_diag_*.py
apps/camoufox-host/lock/camoufox-...-r1-diag-v2-source.json
docs/camoufox-program-status.md
本文
```

禁止修改或执行：Dockerfile、strict_build.py、diag_gate.py、0000–0004、9000、
engine source、Artifact、probe、FP2 comparator、Phase B-2、Phase C-2、
prepare-bound-image 实机消费、Phase D、Browser、V1–V4。

实现必须以全部 no-browser tests、`py_compile`、`git diff --check`、scope diff 与 clean
checkpoint 收口后停止，返回主脑 Gate。实际 durable-root sentinel/reboot qualification
属于 checkpoint Accepted 后的独立 host qualification，不在本离线实现中偷偷执行。
