# Fingerprint Surface Truth Matrix（指纹表面真实能力总账）

- 性质：独立、只读 QA / capability audit。未修改任何产品源码。
- Exact baseline：`0d742906881263796d86018342699046c5e1b0cd`（`origin/baseline/dev`，开工时 fetch/prune 后与任务下发一致）。
- Task branch：`agent/qa/fingerprint-surface-52ebde`。
- 观察对象：Managed Identity Silo `fp-matrix-a`（vault `qa-fingerprint-surface-52ebde`），
  Artifact `identity-a6d662b871505346f45edb5d`（schema `verisilo-camoufox-resolved-identity/v5`，
  preset balanced-zh-cn、direct 网络、targetOs=windows、fontMode=inherit、timezoneMode=fixed、
  browserBinding = FORMAL_R1_V3 deterministic canvas 变体）。
- 观察方式：CLI + real backend（`verisilo-cli create/start/recheck/page goto/page evaluate`，
  真实 Camoufox Managed browser + 真实 Host），loopback QA 观察服务器
  （`probe_server.py`，仅 QA 用，不随产品发布）；原始文件见 `raw/`。
  未使用 Preview/Mock；未 build installer。
- 产品路径链路核对：ManagedIdentityIntent（`domain.rs:691`）→ `provision_managed_artifact`
  （`application/identity.rs:178`）→ Host `--provision-artifact`（`engine.rs:2723`）→
  Artifact v5 → launch 时 `resolvedConfig` 即 CAMOU_CONFIG（Host 强制 sent-minus-allowed-extras
  与 disk 配置零差异，`host_v1.py:1312-1335`；`normalize_camou_config_env`
  `host_runtime.py:465-523`）→ page observation（loopback probe，top window）→
  `reconcile_website_identity`（`identity_policy.py:862`）→ Runtime Identity Evidence v0。

## 层级语义（本报告用法）

- **Configured**：Artifact（resolvedConfig / stableSignalsDeclared / policy）里真实存在该字段。
- **Applied**：该值真正进入浏览器运行时（CAMOU_CONFIG 强制相等链路，或 Playwright/prefs 旁路），且与观察一致可证。
- **Observed**：产品自己的 probe（`probe.html` readIdentity）实际读取到该 surface。
- **Reconciled**：进入正式 Runtime Identity Evidence 的 expected-vs-observed 对账信号。

状态词汇：CONFIGURED / APPLIED / OBSERVED / RECONCILED / UNAVAILABLE / INHERIT / NOT_APPLICABLE / UNKNOWN。

## 主矩阵

| Surface | Configured | Applied | Observed | Reconciled | Stability | Host leak | Evidence |
|---|---|---|---|---|---|---|---|
| userAgent（UA / browser identity） | yes（`navigator.userAgent`） | yes（CAMOU_CONFIG verbatim；request header 同值） | yes（probe + headers + 三 realm 同值） | yes（matched） | stable（两次冷启动同值） | no | observed.json / evidence signals / raw/request-headers.jsonl |
| platform / oscpu | yes | yes | yes（probe + realms） | yes（matched） | stable | no | evidence signals |
| navigator.appCodeName/appName/appVersion/product | yes（config keys） | yes（CAMOU_CONFIG） | probe 读到（`observedFull.app*`），**不在 digest** | no | stable | no | observedFull only |
| language | yes（`locale:*`） | yes（navigator + `Accept-Language: zh-CN,zh;q=0.9`） | yes | yes（matched） | stable | no | evidence signals + request-headers.jsonl |
| languages（数组） | 部分（locale 推导 `[zh-CN, zh]`） | yes | yes（digest 含） | **no**（只比 `language`，数组无 expected） | stable（含 header 相干） | no | digest only |
| timezone / UTC offset | yes（`timezone`） | yes（Intl + Date 三 realm 同值） | yes（timezone + utcOffsetMinutes） | yes（matched） | stable | no | evidence signals |
| screen（8 键） | yes | yes（宿主机真实屏更大，观察 1280×800 证明 spoof 生效） | yes | yes（matched；6 键投影，availLeft/Top 不比——contract 注明） | stable | no | evidence signals |
| devicePixelRatio | **no（v5 契约即 host-bound，无此键）** | INHERIT（引擎原生；观察 1.5 = 宿主显示器） | probe 读到；v5 起**不进 digest** | no（有意排除） | host-bound（不可声明稳定） | **是（按设计）** | policy v5 deltas（`identity_policy.py:20,595`） |
| hardwareConcurrency | yes（=2） | yes（宿主 >2 核，观察 2 证明生效） | yes | yes（matched） | stable | no | evidence signals |
| deviceMemory / userAgentData / windowChrome | UNAVAILABLE（policy 声明 Firefox 不暴露） | UNAVAILABLE | probe 读到 null | no（无 expected） | 不可用（恒定） | no | policy.unavailableFields + observedFull.unavailable |
| maxTouchPoints | **no（不在任何字段集）** | 引擎默认（观察 0；每次启动 BrowserForge 生成、normalize 剥离，不入 Artifact） | probe 读到；不在 digest | no | 未声明（本宿主两次冷启动同 0） | 引擎侧 | `host_runtime.py:477-506`；observedFull |
| webdriver | no（非 Artifact 字段） | yes（引擎默认 false） | probe 读到 + Rust `WebsiteIdentityObservation` + UI「自动化标记」 | websiteIdentity only（不在 identityEvidence） | stable | no | website_identity.rs / observedFull |
| DNT | no（v5 移除；launch 时 BrowserForge 产出被剥离） | 引擎原生（观察 `unspecified`；无 DNT 请求头） | probe 读到；v5 起不进 digest | no（v5 有意排除） | n/a | 引擎原生（按设计） | `identity_policy.py:381-385`；request-headers.jsonl |
| GPC | yes（`managed-opt-out`，=true） | yes（JS true + `Sec-GPC: 1` 请求头） | yes | yes（matched） | stable | no | evidence signals + request-headers.jsonl |
| canvas | yes（`canvas:seed` + binding 选定 deterministic 分类） | yes | probe 读 4 组 hash；**有意不进 digest**（`identity`="stable hard-observed Canvas surface; excluded from ObservedWebsiteDigest v2"） | no（contract 有意排除） | **stable（实测两次冷启动 4 个 hash 全同）** | no | policy.canvasClassification；observedFull.canvas |
| audio | yes（`audio:seed`） | yes | digest 含 `audioHash` | **no**（Artifact 无 expected hash，对账永不比较） | stable（两次冷启动同值） | no | digest only |
| fonts | **inherit（fontMode=inherit；同时声明 82 族字体列表 + spacing_seed）** | 列表注入可用（`injectedFonts` 82 全 available）；**宿主字体不隔离**（hostFontControls 空 = 本宿主恰无外部字体） | probe 读宽度（host-bound，不进 digest）+ controls | no（有意：`fontMode=inherit：字体指标由主机提供，当前不能诚实比较。`） | 本宿主 stable，但语义 host-bound | **是（按设计 inherit）** | evidence signals（fonts=unavailable） |
| voices | yes（53 项 managed + fakeCompletion） | yes（观察 53 == expected） | yes | yes（matched） | stable | no | evidence signals |
| WebGL1 vendor | yes | yes | yes | yes（matched） | stable | no | evidence signals |
| WebGL1 renderer | yes（系列声明 `…, or similar`） | yes（观察同串） | yes | **unavailable by contract**（"Artifact 只声明了渲染器系列，当前 contract 不支持精确比较。"） | stable | no | evidence signals |
| WebGL1 summary | yes（params/attributes/shaderPrecision/extensions） | yes | digest 含 `webglSummary` | no（只比 vendor/renderer） | stable | no | digest only |
| **WebGL2** | 部分（attributes/params/shaderPrecision/extensions 四键；**vendor/renderer 无键**，引擎侧继承 GL1 串） | yes（`webgl2Available=true`，renderer==GL1 串） | probe 读到 webgl2*；**不在 digest** | no | stable | no | observedFull only |
| **WebGPU** | **no（v5 config 完全无此 surface）** | 引擎原生 API 暴露（`navigator.gpu=true`；本宿主 `requestAdapter()`=null） | **产品 probe 完全不读** | no | UNKNOWN（宿主相关） | **潜在（见 P1-2）** | raw/direct-observations.json |
| mediaDevices | yes（counts 1/1/0） | yes（Windows fake-stream prefs + 引擎） | yes | yes（matched；label 收集后不入 evidence） | stable | no | evidence signals |
| geolocation | NOT_APPLICABLE（v5 无键；v6 才有 Playwright override） | no（权限 `prompt`，6s 无结果） | 产品 probe 不读 | no | n/a | 引擎原生 | raw/direct-observations.json |
| WebRTC address | NOT_APPLICABLE（v5 无 `webrtc:*`；v6 才有） | 引擎默认：ICE host 候选全部 mDNS 混淆（`.local`），无裸 IP | 产品 probe 不读（launch 仅有出口 IP 校验，非 JS surface） | no | 未声明 | 引擎默认防护；srflx 反射未测 | raw/direct-observations.json |
| public network exit（网络层） | direct（无代理） | yes（launch 时 `verify_browser_public_address` 经 ipify/ip.sb 校验出口） | launch 级（网络 evidence，非 JS probe） | network evidence 层，非 identityEvidence | 随网络 | n/a（transport boundary，单列） | host_v1.py:679-705 |
| request headers | yes（`headers.Accept-Encoding`） | **yes（实测 document+fetch `gzip, deflate, br, zstd` 与 Artifact 精确一致；UA/语言/Sec-GPC 相干）** | **产品 probe 无任何请求头采集** | no | stable（实测） | no（本次实测未发现泄漏） | raw/request-headers.jsonl（QA 服务器采集） |
| window geometry（outer/inner/screenX/screenY） | outer+screenX/Y 是 config 键 | yes；**小屏时被静默 clamp（工作区 > 请求值时不触发；本宿主未触发）** | probe windowGeometry 读到；不进 digest | no（screenX/Y 声明 session-variable；outer/inner 未声明） | 本宿主 stable | 显示器相关（按 session-variable 分类） | observedFull.windowGeometry |
| history.length | yes | yes | yes | yes（matched） | stable（probe 页本地语义） | no | evidence signals |
| navigator.onLine / documentFontsStatus / windowScreenX/Y | 声明为 session-variable（不入 stable） | yes | probe 读到（session 块） | no（有意） | session | 按设计 | policy.sessionVariableFields |
| TLS / DNS / QUIC（transport boundary） | — | — | — | — | — | — | **UNAVAILABLE**：状态页既有边界（未验证），本次不改变 |

## 四层结论摘要

- **Configured→Applied（A 类缺口）**：未发现。CAMOU_CONFIG 与 Artifact `resolvedConfig` 由
  Host 强制零差异；所有可比对账信号 matched；`headers.Accept-Encoding` 在 document 与
  fetch 两条路径上与 Artifact 精确一致——「配置了但网站没拿到」一例未观察到。
- **Applied→Observed（B 类缺口）**：配置/生效但产品从不观察的 surface：request headers、
  WebGL2、window geometry、WebRTC、geolocation、WebGPU、navigator.app*。
- **Observed→Reconciled（C 类）**：对账语义本身未发现**错误**比较；有的是「进了 digest 但
  永不比较」（audioHash、languages、webglSummary、fontNegativeControls——因 Artifact 无
  expected，代码注释声明这是有意的诚实边界）。
- **跨 realm**：top / iframe / dedicated-worker 三 realm 在 UA、platform、languages、timezone、
  hardwareConcurrency、WebGL vendor/renderer 全部一致，无 drift。注意：正式 probe 只覆盖
  top window（FP2 runner 独立存在、不进 evidence——INTENTIONAL_BOUNDARY）。
- **Fresh recheck**：**在真实 packaged runtime 上失败**（详见下 P1-1）。Core 行为诚实：
  state=unavailable、observedAt 不变、不伪造新证据、launch 证据保留。
- **冷启动稳定性**：两次冷启动（不同 session）`observedWebsiteDigest` 完全一致
  （`sha256:fa00ad84…`），含 canvas 全部 4 个 hash、audioHash、voices、DPR、maxTouchPoints。
  deterministic canvas binding 的「stable hard-observed」分类得到实测支持。

## Gap 分类

- **CONTROL_GAP**：无（未发现 Artifact 配置但网站没有得到的情况）。
- **OBSERVATION_GAP**：WebGL2（已配置、probe 已读、digest/evidence 均无）；request headers
  （已配置已生效、产品零采集）；WebRTC/geolocation/WebGPU（产品零观察）；navigator.app*、
  window geometry（probe 读到但 digest 排除）。
- **EVIDENCE_GAP**：audioHash / languages / webglSummary / fontNegativeControls 在 digest 中
  但对账无 expected，永不进入 evidence；WebGL2 无任何 evidence 投影。
- **COHERENCE_GAP**：window.screenX=2232（provision 时取自宿主桌面位置并写进 Artifact）大于
  spoof 后 screen.width=1280——网站比较 `window.screenX < screen.width` 可见不自洽；
  WebGPU adapter.info（若有 adapter）将与 spoof 的 WebGL GTX 980 串冲突（本宿主未取得
  adapter，未能实证）。
- **INTENTIONAL_BOUNDARY**（按 contract 记录，不判 bug）：canvas 不进 digest（deterministic
  分类明示）；fonts inherit 不进 digest、对账 unavailable；DPR/DNT v5 起退出 stable 契约；
  session-variable 字段排除；正式 probe 仅 top window；TLS/DNS/QUIC unavailable。

## 问题清单（P0/P1/P2/LOW）

**P0**：无。所有可对账 core surface 全部 matched，跨 realm 无 drift，冷启动零漂移。

**P1**
1. **Packaged Host 缺 `reobserve_identity`，Fresh Recheck 在真实 runtime 断裂**。baseline
   `0d74290`（09-13）的 Core 已要求该命令，但本机 staged engine package 的签名 Host exe
   （09-12 staged，与 rc2/rc3 包同源）不含它：`recheck` → `unknown_command` → evidence
   honest `unavailable`。源码级 `test_page_command.py::check_reobserve_identity` 通过。
   Owning seam：engine package 构建/发布流（host lane）需要出含该命令的新包；Core 侧
   缺少协议能力协商（可作第二步）。产品语义：状态页宣称的能力在唯一真实 runtime 路径上
   不可用。
2. **WebGPU 无策略边界（潜在真实 GPU 泄漏）**。引擎暴露 `navigator.gpu`；Artifact/policy/
   probe/digest/evidence 完全没有该 surface。本宿主（VM）`requestAdapter()=null` 未能实证
   泄漏，但在普通 D3D12 硬件上 Firefox 152 大概率返回真实 adapter，`adapter.info`
   （vendor/architecture/description）将与 spoof 的 WebGL GTX 980 直接矛盾。属「证据支持
   的推断 + API 存在已实证」。Owning seam：identity_policy surface inventory（声明
   unavailable/managed）+ Host launch prefs（禁用或伪装）。

**P2**
3. **WebGL2 配置-观察-证据三断层**：四键已配置、probe 已读 webgl2*、digest 与 evidence 均无。
   GL2 是一等指纹面（shaderPrecision/params 已是 Artifact 承诺），成本最低的补法是复用
   webglSummary 通道。
4. **Request headers 无产品观察路径**：`headers.Accept-Encoding` 已配置已生效，但产品从不
   采集请求头证据（QA 靠外部服务器验证）。
5. **window.screenX/Y 的 provision 期宿主绑定**：把宿主窗口坐标写进 Artifact 并应用，可产生
   screenX > screen.width 的可见不自洽（且 outer* 存在未声明的工作区 clamp 路径）。
   建议按 session-variable 处理或钳制进 spoof screen 范围。

**LOW**：audioHash/languages/webglSummary/fontNegativeControls 有 digest 无对账（契约注释
已声明）；navigator.app* digest 排除；maxTouchPoints 游离于所有字段清单（本宿主稳定为 0）；
UI 中 matched 可与 unavailable 信号共存（信号 chips 可见、代码有明确设计注释，但
CurrentSessionIntegrity 的 identity 行仍计为 ok）。

## 下一批值得 implementation 的 gap（1–3 个）

1. **出含 `reobserve_identity` 的新 engine package 并推进 staging**（解除 P1-1；可选加协议
   能力协商）。这是状态页已宣称功能的最后一公里。
2. **WebGPU 边界决策与实现**（P1-2）：默认 `dom.webgpu.enabled=false`（或适配器 info 伪装/
   拒绝），并在 policy/UNAVAILABLE 与 probe 中如实声明——二选一，须先显式决策。
3. **把 WebGL2 + request headers 纳入产品观察/证据**（P2-3/4）：probe 已读 GL2、header 配置
   已存在，最小扩展即可消除两个「已承诺但零证据」的断层。

## Fonts / Canvas 专项结论（绑定 exact baseline `0d74290`）

- **Fonts = inherit / host-bound，确认**。`policy.fontMode=inherit`；probe 注入
  `window.__probeFonts/FontUniverse/HostFonts`，本宿主 `hostFontControls=[]`
  （`controlsTested=0`，inherit 模式不执行 masking 失败判定）；宽度证据不进 digest；
  evidence `fonts=unavailable`（诚实）。另一并行 Agent 若在此期间推进 Managed Font
  Isolation，不影响本结论——本结论只绑定 `0d74290`；其余 surface evidence 在未来
  integration 后仍然有效。
- **Canvas**：本 Artifact 的 browserBinding 命中 FORMAL_R1_V3 deterministic 变体，
  分类为「stable hard-observed Canvas surface; excluded from ObservedWebsiteDigest v2」；
  实测两次冷启动 rawHash/exportHash/pngBytesHash/decodedPngPixelsHash 全部相同。
  「不进 digest」是 contract 行为，**不是 bug**；同时按当前 contract，canvas 也没有进入
  formal evidence 的对账信号（hard-observed 但无 expected 值）——已在 EVIDENCE_GAP 注记。

## 边界与未验证项

- v6/network-bound 路径（geolocation override、`webrtc:ipv4/6`、networkIdentity 一致性）未做
  真实代理运行观察（本审计实例为 direct/v5）；相关行按代码与契约记录为 NOT_APPLICABLE/代码级。
- WebRTC srflx/STUN 反射、TLS ClientHello、DNS 路径、QUIC：未测（后者为状态页既有
  unavailable 边界）。
- WebGPU adapter.info 泄漏在无 adapter 宿主上无法实证，P1-2 为「API 存在实证 + 泄漏推断」。
- 运行环境：本机 dev staged package（签名 camoufox-host.exe，09-12）；非 installer 安装路径。
