# VeriSilo Companion data-flow inventory

- 状态：**商店披露的机械事实基线**
- 生成：2026-08-15，基于 `main@45bfea1` 的 extension 源码逐项核对
- 用途：Chrome Web Store / Edge Add-ons 隐私表单与隐私政策的唯一事实来源。任何 "local only"、"no data collected" 表述必须能被下表覆盖。

## 1. 触发模型

没有任何自动运行的数据收集。全部动作由用户在 side panel 中显式触发：

| 动作                                          | 触发方式                                            | 是否默认发生 |
| --------------------------------------------- | --------------------------------------------------- | ------------ |
| 页面扫描（observation）                       | 用户在目标网页点击工具栏图标打开侧栏 → 点 Scan      | 否           |
| MAIN-world 观察                               | 扫描的一部分（`executeScript`，best-effort）        | 否，随扫描   |
| Network Check                                 | 用户点击 → 授权 optional host permission → 点击确认 | 否           |
| Labs 网页泄漏实验                             | 用户显式开启                                        | 否           |
| Privacy controls（WebRTC/network prediction） | 用户显式开启，可恢复                                | 否           |
| 报告导出（JSON/HTML）                         | 用户点击导出并确认                                  | 否           |
| 打开项目页                                    | 用户点击 desktop project 按钮                       | 否           |

## 2. 页面扫描读取的信号（`content.ts` 的 `collect` 清单）

全部读取发生在当前用户主动扫描的那一个标签页，且只读不改：

| 信号组                           | 内容                                                                                        | 敏感级 | 稳定级  |
| -------------------------------- | ------------------------------------------------------------------------------------------- | ------ | ------- |
| `navigator`                      | userAgent、platform、language、languages、hardwareConcurrency、deviceMemory、maxTouchPoints | medium | stable  |
| `ua_ch`                          | User-Agent Client Hints                                                                     | medium | stable  |
| `timezone`                       | `Intl` 解析的时区                                                                           | medium | stable  |
| `screen`                         | 屏幕尺寸、avail 尺寸、色深                                                                  | medium | session |
| `canvas_hash`                    | Canvas 指纹摘要                                                                             | high   | session |
| `webgl`                          | WebGL renderer/vendor 摘要                                                                  | high   | session |
| `webgpu`                         | WebGPU 摘要                                                                                 | high   | session |
| `audio`                          | AudioContext 指纹摘要                                                                       | high   | session |
| `fonts`                          | 字体枚举摘要                                                                                | high   | session |
| `media_devices`                  | 设备枚举摘要                                                                                | high   | session |
| `permissions`                    | 站点权限状态摘要                                                                            | medium | session |
| `storage`                        | `cookiesEnabled` 等存储状态                                                                 | medium | session |
| `webrtc`                         | WebRTC 摘要                                                                                 | high   | session |
| `window_iframe`                  | iframe 上下文观察                                                                           | medium | session |
| `dedicated_worker`               | Dedicated Worker 自测                                                                       | medium | session |
| `main_world_navigator_untrusted` | 页面可见的 navigator（显式标注 `page_observable_untrusted`）                                | medium | stable  |

**不读取**：Cookie 值、LocalStorage/IndexedDB 内容、网页正文、表单输入、密码、浏览历史。

## 3. 数据存放位置（全部本机）

| 存储                     | 内容                                                            | 保留策略                               | 访问限制                                     |
| ------------------------ | --------------------------------------------------------------- | -------------------------------------- | -------------------------------------------- |
| 内存（content script）   | 当前扫描中的信号                                                | 页面导航/关闭即失                      | 页面隔离世界                                 |
| `chrome.storage.session` | 当前报告（`report:<tabId>`）、Network Check 结果与 handoff 状态 | session 生命周期；UI 有清除按钮        | `setAccessLevel("TRUSTED_CONTEXTS")`         |
| `chrome.storage.local`   | 最多 20 份脱敏历史报告（`saved-report:*`，无痕模式不保存）      | 30 天 TTL，写入前裁剪                  | `setAccessLevel("TRUSTED_CONTEXTS")`，不同步 |
| 扩展外（桌面端 Vault）   | 用户提交的 Network Check 结果                                   | 见 `docs/store-disclosure.md` 第 10 条 | 仅当 Vault 解锁且 Silo 匹配                  |

## 4. 网络请求（仅 Network Check，用户触发）

| Endpoint                                                                                    | 请求                                              | 发送内容                        | 收到的内容                | 保留                           |
| ------------------------------------------------------------------------------------------- | ------------------------------------------------- | ------------------------------- | ------------------------- | ------------------------------ |
| `https://ipwho.is/`                                                                         | GET，`credentials: omit`，`no-referrer`，10s 超时 | 无请求体；第三方看到请求来源 IP | 出口 IP/地理位置/ASN JSON | 存入 `storage.session`，可清除 |
| `https://cloudflare-dns.com/dns-query?name=example.com&type=A&do=true`                      | GET，`Accept: application/dns-json`               | 同上                            | 固定域名 A 记录           | 同上                           |
| `https://dns.google/resolve?name=example.com&type=A&do=true&edns_client_subnet=0.0.0.0%2F0` | GET                                               | 同上                            | 固定域名 A 记录           | 同上                           |

- 请求绝不自动发出；授权 optional host permission 之后仍需用户确认才发。
- 响应限制 64 KiB，JSON 解析后构建结果。
- 三个域名之外无任何网络调用（bundle gate 逐 URL 校验）。
- 打开项目页导航到 `https://github.com/QianQIUlp/VeriSilo`（`packages/contracts/src/product.ts`）。

## 5. Native Messaging（`io.verisilo.host`）

仅当本机装有 VeriSilo 桌面端且 Host 已注册时可用；失败一律降级为 `local_only`，不影响扩展使用。

| 消息                      | 载荷                                                                       | 方向                                            |
| ------------------------- | -------------------------------------------------------------------------- | ----------------------------------------------- |
| `get_runtime_status`      | `protocolVersion`、`requestId`（UUID）                                     | 扩展 → Host → 返回脱敏快照（45s 过期）          |
| `submit_network_evidence` | `siloId`、`runtimeId`、`networkCheck`（刚产生的本地结果）、`coverage` 声明 | 扩展 → Host（临时 inbox，16 KiB/32 条/10 分钟） |
| `open_desktop`            | 无参数                                                                     | 扩展 → Host → 唤醒 `verisilo.exe`               |

**永不发送**：Cookie、LocalStorage/IndexedDB、凭据、浏览历史、完整 observation report、Vault 密钥。快照不含消息、代理标签、Silo 元数据和浏览器自有数据（见 `docs/architecture.md` 第 58 行）。

## 6. 导出（用户触发）

JSON/HTML 导出默认脱敏高敏感信号值（`report-export.ts` 的 `redactObservationReport`）；文件名含 reportId。

## 7. 商店问题映射

| 商店问题         | 答案依据                                                                                  |
| ---------------- | ----------------------------------------------------------------------------------------- |
| 收集什么数据     | 第 2 节信号清单；全部经用户触发的扫描                                                     |
| 数据是否离开本机 | 仅 Network Check 的三个请求（第 4 节）与可选的原生桥接（第 5 节）；observation 数据不上传 |
| 远程代码         | 无。bundle gate 校验无 `eval`/`new Function`/动态 `import`                                |
| 是否出售/共享    | 否。无任何 VeriSilo 服务器                                                                |
| 保留与删除       | 第 3 节；用户可随时清除 session 结果与本地历史                                            |
