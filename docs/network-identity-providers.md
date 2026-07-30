# VeriSilo 网络身份与出口 Provider

## 产品原则

网络身份不是一个“IP 纯净度”分数。VeriSilo 把它拆成可核查的事实：Silo 绑定了什么、端点是否可达、认证是否经过协议验证、浏览器是否应用了路由、当前页面实际看到哪个出口，以及 DNS/WebRTC 证据覆盖到哪里。

- 默认本地优先，不运营默认公网代理或验证服务。
- 用户可以使用自有 HTTP/HTTPS/SOCKS 代理、住宅/机房/VPS 出口，或自己运行的 Mihomo/Clash 兼容内核。
- 长期 Silo 默认固定端点或固定 Mihomo 节点；轮换必须是用户明确修改，不做后台随机切换。
- “必须代理”不含 `DIRECT` 回退。端点、认证或 Mihomo 节点绑定失败时，Silo 拒绝启动或浏览器网络自然失败。
- IP、DNS、WebRTC、时区和语言分别显示证据，不把公共 DoH 对比误称为 DNS 泄漏证明。

## 市场需求结论

公开资料中成熟产品的共同基础不是“每个 Profile 自动购买一台 VPS”，而是每个 Profile/Silo 绑定一个用户自有或平台提供的代理出口，并提供导入、检查、地理信息协调和明确轮换。可参考 [GoLogin proxy FAQ](https://support.gologin.com/en/articles/14839275-faq-proxies)、[Multilogin proxy FAQ](https://multilogin.com/help/en_US/all-about-multilogin-proxy/multilogin-proxy-faq)、[AdsPower 全局设置](https://help.adspower.com/docs/global_settings)、[Dolphin Anty proxy checker](https://docs.dolphin-anty.com/en/working-with-proxies/check-proxy-in-dolphin-anty)、[Incogniton proxy integration](https://docs.incogniton.com/proxy-management/integrating-proxies) 和 [Octo Browser profile settings](https://docs.octobrowser.net/en/profiles/browser-profile-settings/)。

VeriSilo 采用同样已经被验证的入口需求，但不复述供应商关于匿名性、信誉或风控绕过的营销结论。Mihomo 只负责转发和规则；真正改变公网 IP 的仍是用户选择的远端节点或代理服务。

## 当前实现范围

| 阶段                            | 当前状态         | 已实现                                                                                                                                                        | 明确未宣称                                                                                                                    |
| ------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| 第一阶段：固定代理              | 已实现           | 一行导入、每 Silo 固定端点、Vault 凭据、端口/协议认证预检、随机 loopback 中继、fail-closed 启动参数、分层证据                                                 | HTTP 认证只有真实请求后才能最终验证；HTTPS/SOCKS4 上游认证不由内置中继处理                                                    |
| 第二阶段：外部 Mihomo           | 已实现可用适配器 | 连接用户已运行的本机 Controller、读取选择组/节点/最近延迟及可用的 Provider 更新时间、加密 Controller Secret、每次启动前选择并回读确认、浏览器只连随机本机端口 | 不读取或托管机场订阅；订阅导入/更新仍由用户的 Mihomo 客户端负责；不捆绑 GPL 内核；Controller 不提供结构化地区时不从节点名臆测 |
| 第三阶段：随包内核/WSL/TUN/并行 | 接口与只读检测   | WSL 可用性/发行版只读检查，Provider 边界和发布门槛                                                                                                            | 尚未分发 Mihomo、未改系统路由、未启用 TUN、未启动 WSL Chromium、未允许多 Silo 并行                                            |

## 第一阶段数据流

```text
用户粘贴 host:port[:user:password]
  ├─ 非秘密端点 → Silo 网络配置（加密 Vault 内）
  └─ 用户名/密码 → 独立随机引用 → 加密 Vault 内

启动 Silo
  ├─ 校验 Profile 锁和网络模型
  ├─ 检查代理端点
  ├─ SOCKS5：完成认证协商（可在访问网站前验证凭据）
  ├─ HTTP：确认端点可达；认证保留“已配置”，等待真实出口请求验证
  ├─ 启动随机 127.0.0.1 SOCKS5 中继
  └─ Chrome/Edge 只收到 loopback 地址，不收到上游用户名或密码
```

必须代理的 Chrome/Edge 启动配置包含：

- 单一代理地址，不加入 `direct://`；
- `--proxy-bypass-list=<-loopback>`，取消 Chromium 隐式 loopback 绕过；
- `--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE <proxy>`，让普通页面域名不走宿主 DNS 解析；
- `--disable-quic`，避免 QUIC 绕过当前 TCP 代理链路；
- `--webrtc-ip-handling-policy=disable_non_proxied_udp`，限制非代理 WebRTC UDP。

这些参数是启动级防护，不等于 TLS、DNS 或 WebRTC 已获得包级证明。Chrome SOCKS/DNS 行为依据 [Chromium SOCKS proxy design](https://chromium.googlesource.com/website/%2B/refs/heads/main/site/developers/design-documents/network-stack/socks-proxy/index.md) 和 [Chromium proxy documentation](https://chromium.googlesource.com/chromium/src/%2B/master/net/docs/proxy.md) 持续回归。

## 第二阶段外部 Mihomo 适配器

用户先在自己的 Mihomo/Clash 兼容客户端中导入和更新机场订阅。VeriSilo 只连接明确填写的 `http://127.0.0.1:<port>/` 或 `http://[::1]:<port>/` Controller：

1. `GET /proxies` 读取选择组、当前节点、可选存活状态和最近延迟；可选读取 `/providers/proxies` 的 Provider 名称、节点数和更新时间；
2. 用户把选择组与节点固定到 Silo；
3. Controller Secret 以随机引用加密保存；
4. 每次启动前再次确认节点仍存在；
5. `PUT /proxies/<group>` 选择节点，再次 `GET /proxies` 回读；
6. 端点协议检查成功后才启动浏览器；
7. Mihomo 或本机中继崩溃时没有宿主直连回退。

节点“地区”最终以 Silo 内真实出口检查返回的国家/城市为证据。节点名中的旗帜、缩写和营销名称没有统一 API 语义，因此只原样展示，不自动当作地理事实。

本次不接受通用“换 IP URL”。这类 URL 往往同时承担凭据、状态写入和任意目标请求；在没有逐 Provider 的域名白名单、方法/响应协议、SSRF 防护和审计记录前，简单保存并后台访问会扩大风险。未来加入时，URL 和令牌必须进入 Vault，调用必须是用户明确操作，结果仍要由 Silo 内实际出口复查。外部 Mihomo 的订阅 URL 则继续由用户自己的客户端保存和更新，避免 VeriSilo 重复接管订阅密钥。

远程 Controller 被拒绝，以免 VeriSilo 变成局域网/公网 SSRF 或远程代理管理入口。Controller API 行为参考 [Mihomo API](https://wiki.metacubex.one/en/api/)；节点与订阅仍属于用户自己的外部进程。

## 证据状态

桌面端本次启动卡片依次展示：

1. 网络配置；
2. Mihomo 节点绑定（如适用）；
3. 代理端点；
4. 代理认证；
5. 浏览器路由；
6. Silo 实际出口；
7. DNS 路径证据；
8. WebRTC 路径。

桌面控制器自己的 IP 请求不能替代 Silo 证据。实际出口检查由用户在已启动 Silo 内的 Companion 发起。它可以确认该浏览器环境本次请求看到的公网 IP、ASN、地区和出口时区，并与浏览器时区给出一致性建议。

当前“公共 DNS”检查只是向 Cloudflare 和 Google 公共 DoH 查询固定域名并比较答案。它不能看到操作系统、路由器、运营商或代理实际使用的递归解析器，因此不能证明“无 DNS 泄漏、污染或劫持”。真正的 DNS 路径验证需要用户自托管唯一 canary 域名及权威 DNS 日志，或未来 VM/远端 Agent 内的受控测试。

WebRTC 当前能验证浏览器隐私设置的控制权和回读结果，但没有默认公共 STUN 服务或包级观测，因此不会显示“实际 WebRTC 出口已证明”。

## 第三阶段为什么拆分发布

- Mihomo 项目采用 GPL-3.0；随包分发需要独立组件边界、源码/许可证义务、SBOM 和更新链路审计。
- TUN 会修改系统路由、DNS 和防火墙语义，通常涉及管理员权限；卸载和崩溃恢复必须先证明不会断网或泄漏。
- WSL Chromium Provider 需要处理发行版安装、GUI、GPU/音频/剪贴板集成、Windows/WSL 网络模式与浏览器更新。当前只允许固定参数的只读检测，不执行任意 Linux 命令。
- 多 Silo 并行需要每实例端口、进程、Vault 解锁生命周期和出口证据归属都可验证；在这些接口冻结前继续保持单 Silo。

WSL 是可替换 `EnvironmentBackend`，不是 VeriSilo 的硬依赖；完整设计仍遵循 [环境实现路线](environment-roadmap.md)。

## 发布验收

- 凭据、Controller Secret 和未来敏感 URL 不出现在启动参数、日志、明文 Vault envelope 或导出报告中。
- 必须代理配置拒绝 bypass rules，且代理进程/端点停止后测试页面无法访问公网。
- SOCKS5 错误用户名/密码在启动前失败；HTTP 错误认证在 Silo 内出口检查时显示验证失败，不误报。
- Mihomo Controller 只接受 loopback，未知组/节点、401、超时和节点回读不一致都阻止启动。
- 桌面控制器检查与 Silo 内检查使用不同标签，不能互相冒充。
- DNS/DoH 和 WebRTC 的覆盖边界在中英文 UI、商店说明和导出报告中一致。
