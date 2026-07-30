# V0.9：用户自托管远程环境控制面

V0.9 现在包含一条可执行的自托管控制链：Windows 桌面端通过普通 PKI 校验并叠加证书或 SPKI SHA-256 pin 的 HTTPS，与用户自己运行的 Linux/Unix Remote Agent 配对。客户端凭据、远程绑定和单调序列保存在桌面加密 Vault；Agent 在本地以 `0600`、`flock`、同目录原子替换以及文件/目录 `fsync` 保存认证和环境控制状态。

这条链不等于“远程浏览器已经可用”。仓库没有随附真实 VM/容器/浏览器 Provider，也没有画面媒体流。默认部署样例把九项生命周期能力全部声明为 `unavailable`；只有运营者安装并固定一个真正实现能力的 Provider 后，Agent 才能诚实地公布对应能力。

```text
VeriSilo Desktop
  ├─ Vault：TLS pin、短期客户端凭据、Silo 绑定和单调序列
  └─ reqwest/rustls：PKI + 主机名 + 证书有效期 + 强制 pin
          │  HTTPS POST /，协议 v1，≤64 KiB
          ▼
User-operated Remote Agent（Linux/Unix）
  ├─ 一次性配对 token 与短期 bearer 凭据
  ├─ TTL、重放账本、活动、授权与 typed Provider 删除回执
  ├─ 固定 SHA-256 的 stdio Provider 边界
  └─ Provider/Guest（本仓库尚未随附）
       ├─ VM/进程、加密卷与浏览器
       ├─ 出口、DNS、WebRTC 证据
       └─ 画面与输入传输
```

## 信任与传输边界

桌面端只接受：

- `ownership: user_self_hosted`；
- 无用户名、密码、path、query 或 fragment 的 HTTPS origin；
- 非零、小写 SHA-256 证书 pin 或 SPKI pin；
- 普通 Web PKI 校验成功后，同一个已认证 leaf certificate 再通过 pin；
- HTTP/1.1、禁止 redirect、禁止读取系统代理、受限连接/读写超时和 64 KiB 上限。

服务端只开放 rustls listener，不提供明文 HTTP 或证书挑战 fallback。它每条连接只接受一个 `POST /`，要求 `application/json`、精确 `Content-Length` 和 `X-VeriSilo-Protocol: 1`，拒绝 chunked、upgrade、重复 header、Cookie、Proxy-Authorization 和畸形 bearer。配置、证书、私钥和状态路径必须是 canonical absolute path；配置不得 group/world writable，私钥和既有状态必须是 `0600`。

这里的“双向认证”是两种机制组合：服务端由 PKI + pin 向桌面端证明身份，桌面端用配对后签发的短期 bearer 凭据向 Agent 证明身份。当前不是 client-certificate mTLS。证书签发、DNS、续期、防火墙和公网入口限流由自托管运营者负责。

## 明确配对与 Vault

运营者在 Agent 停止时执行：

```bash
verisilo-remote-agent init-token \
  --config /etc/verisilo-agent/server.json \
  --lifetime-seconds 300
```

命令要求交互式终端，拒绝把明文 token 重定向到文件。磁盘只保存域分离 SHA-256 digest；token 最长五分钟且只能消费一次。桌面 UI 要求用户勾选授权，并提交 token ID、到期时间和 token。成功响应只把新客户端凭据返回一次；Tauri 命令将它直接写入 Vault，不返回 WebView、日志或报告。

配对状态同时冻结 server ID、credential ID、节点所有者、数据地区、密钥归属、费用说明、九项能力和 client/server sequence。每次请求都使用新 UUID、nonce、时间戳和严格递增 sequence；响应必须匹配 request ID、时间窗、server ID 和更高 sequence。每个 lifecycle 结果还携带 Agent 持久记录中的 `lastActivityAtUnixMs`，桌面把它同步到稳定 binding 并明确展示。撤销配对会清除桌面 Vault 中的应用凭据、人类/自动化授权和屏幕通道元数据；稳定 Silo 绑定与审计回执会特意保留，供重新配对后恢复或显式删除使用，且绝不会被描述为远端资源已经删除。运营者也可停止 Agent 后执行 `revoke-all` 使服务端凭据全部失效。

### TLS pin 安全轮换

证书或 SPKI 换钥不需要先破坏稳定 Silo 绑定。桌面端提供单独的“安全轮换”动作，并同时要求：

- 旧配对凭据仍存在且未过期；
- 客户端先在旧 pin 的 PKI HTTPS 通道中用旧 bearer 完成一次固定、带 sequence/nonce 的轮换授权；
- 新 endpoint 与旧 endpoint 是同一个 HTTPS origin，且 pin 确实不同；
- 用户单独确认本次轮换；
- 使用由新 pin 下 Agent 签发、最长五分钟的新一次性 token；
- 新配对响应的 `serverId` 与旧配对以及每一条稳定绑定完全一致。

客户端先只把新 token ID 写入加密 Vault 的本地 replay ledger，然后向旧 endpoint 发送 `authorize_tls_pin_rotation`。该请求只包含旧 credential ID、新 pin 与 token ID，绝不包含 token secret；Agent 原子验证旧 bearer、request sequence/nonce 及尚未消费的 token ID，并持久化一个最长 60 秒、绑定 credential/token/new-pin 的单次 challenge。只有这一步通过，客户端才连接新 pin，并在配对请求中提交同一 token ID、token secret 和 challenge。Agent 必须原子消费完全匹配且未过期的 challenge，新配对返回的 `serverId` 还必须与旧授权和全部 bindings 一致。

旧授权失败时客户端绝不会联系新 endpoint。网络失败、Agent 拒绝、错误 server、origin 替换、旧凭据过期，或最终 Vault 提交失败时，旧 endpoint、旧 credential identity 和全部 bindings 都保持不变，但 token ID 仍不可重用；已经认证发送的旧请求会保留递增 replay sequence，避免旧 bearer 之后重用序号。成功时，新 endpoint、新 pairing 和每条 binding 的 endpoint 只通过一次 Vault 原子替换共同生效；所有本地人类会话、自动化授权和 screen channel 同时清空，避免新应用凭据继承旧 bearer capability。部署必须让旧 pin 授权与新 pin 配对在这一分钟窗口内依次可达。

## 生命周期与持久状态

协议固定九项操作：

| 操作               | 额外条件                                 | 成功证据                                                           |
| ------------------ | ---------------------------------------- | ------------------------------------------------------------------ |
| `create`           | TTL、费用确认、网络策略                  | 固定 binding/environment ID、加密卷 attestation、来宾证据          |
| `start`            | 已有绑定；必须代理时证据仍有效           | 来宾证据与 started 状态                                            |
| `stop`             | 已有绑定                                 | stopped 状态                                                       |
| `pause`            | Provider 明确支持                        | paused 状态                                                        |
| `snapshot`         | Provider 明确支持                        | snapshot receipt                                                   |
| `destroy`          | 新删除需单独确认；既有回执恢复不发起删除 | 与 server/Silo/binding/environment/volume/key 绑定的 Provider 回执 |
| `configureNetwork` | 固定策略 ID                              | 新策略与来宾验证证据                                               |
| `health`           | 已有绑定                                 | 远端来宾健康和网络证据                                             |
| `logs`             | 1–200 条、服务端 cursor                  | 有界、脱敏日志                                                     |

`create` 只有在 `destroy` 同样可用时才能公布为可用，以便 TTL 到期后存在真实清理路径。Agent 在启动时及每 30 秒执行 TTL sweep；删除失败不会伪造回执，仍会在后续 tick 重试。新发起的删除只有 `confirmDestroy: true` 才能调用 Provider；`confirmDestroy: false` 只是查询同一 binding/environment 是否已有持久化删除回执，活动环境必定拒绝。TTL、Provider policy 或用户确认删除后，Agent 可重复返回完全相同的 `proofId`、Provider receipt、删除时间、原因和资源清单；客户端不再以“两分钟 freshness”错误拒绝旧回执，而是核对当前 pinned server、持久环境记录中的 proof ID、Silo/binding/environment、原 volume/key 及 typed resource dispositions。只有全部一致才解除本地 binding。

Provider 删除响应不再接受任意 `resourcesDeleted` 字符串或单一 `volumeKeyDestroyed` 布尔值。严格 wire schema 要求以下四种 kind 各出现一次，重复、缺项、未知 kind/status、nil/wrong ID 和旧字段全部 fail closed：

- `compute_instance`：`deleted`，ID 必须等于 remote environment ID；
- `persistent_volume`：`deleted`，ID 必须等于原 volume ID；
- `snapshot`：`deleted` + 非 nil ID，或无快照时明确 `not_applicable` 且没有 ID；
- `ephemeral_key`：`deleted`，ID 必须等于原 key ID。

桌面把它称为“已认证的 Provider 删除回执”：它证明 pinned 自托管 Agent 返回了与本地身份绑定的 Provider 声明，不冒充第三方独立审计、云厂商账单或外部可验证销毁证明。旧版任意字符串删除状态不会被宽松迁移；严格反序列化会拒绝并要求运营者恢复/重新核对。

Agent 的认证状态和控制状态分别持久化，均拒绝未知字段、超限集合、回退 sequence 和重复 request ID/nonce。状态替换先同步临时文件，再原子 rename 并同步父目录；如果 commit 点附近出现无法确定的持久化错误，store 进入 poisoned 状态并拒绝继续变更，直到重启重读磁盘。

Agent 状态不是浏览器 Profile，也不是加密卷本身。它只保存控制元数据、授权、重放账本、活动和已认证的 typed Provider 删除回执；真实 Profile 加密与密钥销毁仍由固定 Provider 执行，其回执不是第三方独立审计。

### 灾难恢复强制分离

正常清理必须继续使用 `destroy`，并且只有核对 server、Silo、binding、remote environment、volume、ephemeral key 和四项 typed Provider dispositions 后才移除本地绑定。若旧 pin、凭据与 Agent 均无法恢复，桌面提供最后手段“强制分离”，但要求两项独立确认：

1. 用户确认只移除本地稳定绑定；
2. 用户确认远端环境、卷和其他资源可能仍存在、运行并继续计费。

强制分离不联网、不调用 `destroy`、不生成或复用 deletion proof，也绝不把本地移除称为远端删除。Vault 会永久保存独立的 orphan receipt：receipt、Silo、binding、remote environment、server、当时 endpoint/pin 和分离时间。该回执允许随后永久删除本地 Silo，但回执不会随 Silo 删除，仍在远程面板中可见，供用户联系自托管运营者清理与核账。

## Provider 边界与网络证据

生产 Agent 可配置一个本地 stdio Provider。其 executable path 来自运营者的 `0600`/受控配置，启动前重新 canonicalize 并核对完整 SHA-256；Agent 直接启动该文件，仅传固定 `--verisilo-provider-v1`，通过有界 stdin/stdout JSON 交换。远程请求不能选择 executable、shell、参数、文件路径、镜像、host、port 或 URL。

必须代理的 `create`、`start`、`configureNetwork` 和 `health` 只有在以下证据全部新鲜且绑定正确时才能成功：

- exact policy 已在来宾执行；
- 出口地址已由来宾观察；
- DNS resolver 与 leak 结果已由来宾观察；
- WebRTC candidate 与 leak 结果已由来宾观察；
- guest agent 版本和健康检查有效。

缺失、过期、UUID 不匹配、`unavailable`、失败或检测到泄漏都必须失败关闭。桌面端也在发送 `start` 前检查所存证据，但这不能替代服务端和 Provider 在浏览器进程启动前的同一规则。

仓库中的默认 `provider.mode = "unavailable"` 不生成任何 VM、卷、出口、DNS、WebRTC、日志、Provider 删除回执或屏幕流。部署步骤和严格示例见 [`crates/verisilo-remote-backend/DEPLOYMENT.md`](../crates/verisilo-remote-backend/DEPLOYMENT.md)。

## 人类会话、自动化与画面输入

桌面端和 Agent 已实现六项类型化交互命令：开启/关闭人类会话、授予/撤销自动化、打开屏幕通道、发送有界键盘/鼠标/文本事件。

- 人类会话需要用户明确确认，并优先于自动化；活跃人类会话期间自动化输入被拒绝。
- 自动化必须单独确认、限定 scope 和有效期；Agent 返回的 scope/期限不得扩大桌面请求。
- screen channel 的 authorization、environment 和到期时间必须受父授权约束。
- 输入事件没有 shell、脚本或任意命令字段。

当前 `openScreen` 只返回经过授权的 channel metadata。桌面 UI 可以展示、刷新和关闭授权，也可以调用类型化输入命令，但不解码或渲染视频/音频；Provider 也尚未随附真实的加密媒体/输入传输。因此“远程桌面画面可用”仍是部分实现，而不是已验证能力。

## 自动证据与发布产物

仓库自动测试覆盖协议版本、未知字段/大小限制、重放与 sequence、配对 token/credential、pin 轮换的全 binding 原子替换、错误 server/origin/过期凭据/transport 失败、token replay、最终 Vault 写入失败、灾难恢复 orphan receipt、持久化重启、TTL sweep→无新删除确认取回旧 Provider 回执→解除 binding、伪造 server/environment/proof/reason、typed resource 缺项/重复/未知 kind/status、last activity、能力协商、必需代理失败关闭、人类优先、自动化 scope、screen/input 授权、HTTP parser、TLS pin 和固定 Provider bridge。

可复核命令：

```bash
cargo fmt --manifest-path crates/verisilo-remote-backend/Cargo.toml -- --check
cargo test --locked --all-targets \
  --manifest-path crates/verisilo-remote-backend/Cargo.toml
pnpm remote-agent:verify
pnpm release:self-test
```

`Self-hosted Remote Agent candidate (Linux x64)` workflow 构建 unsigned Linux x64 operator candidate，附带部署样例、SBOM、依赖许可证证据、SHA-256 和 provenance。它只上传 CI artifact，不部署服务、不创建公网端点，也不把 unsigned artifact称为正式发布。

## 仍需外部或后续实现的门槛

以下项目不能由当前本地仓库测试伪造为完成：

1. 一个经过审查并锁定制品哈希的真实 VM/容器/浏览器 Provider，以及其合法基础镜像。
2. Provider 内真实的加密持久卷、每环境网络栈、浏览器 Profile、来宾 Agent 和删除密钥流程。
3. 真实 authenticated screen media/input transport；当前只有授权协议与 channel metadata。
4. 用户域名、DNS、有效证书、续期、防火墙、入口限流，以及在真实证书/SPKI 换钥中的轮换与回滚演练。
5. 在真实 WAN 上完成配对、创建、断线、重启、并发、TTL、代理故障、证据、销毁和恢复矩阵。
6. 对 Agent/Provider/来宾边界的独立安全审计和运营恢复演练。

这些门槛完成前，UI 和发布说明必须把 V0.9 称为“可执行的自托管控制面 + 待安装真实 Provider”，不得称为已经交付的云浏览器、完整远程桌面或不可检测环境。
