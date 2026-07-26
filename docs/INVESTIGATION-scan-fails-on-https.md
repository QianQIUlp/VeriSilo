# 调查报告：扩展扫描当前页面在 https 站点上无结果

> 状态：已在 VeriSilo Companion 0.1.2 后续实现中修复。本文保留当时的复现、根因和验收依据；其中 Manifest 片段记录的是 0.1.0 基线，不代表当前权限配置。

复现环境：Edge，VeriSilo Companion v0.1.0，页面 https://dash.cloudflare.com/...。仅调查，未改任何代码。

## 现象

用户在 https 的 Cloudflare 页面打开侧栏并点「扫描当前页面」，「当前结果」始终显示「尚未扫描」，顶部出现英文 VeriSilo only scans regular HTTP(S) pages after you request it（被用户红圈标出）。页面明明是 https。

## 根因

那条英文报错的真实触发条件不是「页面非 http(s)」，而是「读不到 tab.url」。代码在 apps/extension/src/background.ts:128 的 scanCurrentTab：

```ts
if (tab.url === undefined || !/^https?:/u.test(tab.url)) {
  throw new Error(
    "VeriSilo only scans regular HTTP(S) pages after you request it.",
  );
}
```

在 MV3 中，当扩展既没有 tabs 权限、也没有该 origin 的 host 权限时，chrome.tabs.query 返回的 tab.url 会被剥成 undefined。

manifest（apps/extension/manifest.json）：

```json
{
  "permissions": [
    "storage",
    "sidePanel",
    "activeTab",
    "nativeMessaging",
    "scripting"
  ],
  "optional_permissions": ["privacy", "proxy", "declarativeNetRequest"],
  "optional_host_permissions": ["http://*/*", "https://*/*"]
}
```

三个关键事实：

1. 只有 activeTab。它是临时权限，侧栏按钮点击不构成新的 activeTab 手势；service worker 里 tabs.query 走普通路径，没有 tabs 权限时 url 即 undefined。
2. optional_host_permissions 声明了 http/https 全量，但全仓没有任何代码调用 chrome.permissions.request({ origins: [...] }) 来实际请求它们。
3. request_optional_privacy_permission 只请求 privacy，和扫描需要的 host 权限无关。

所以扫描在注入 content.js 之前就因 tab.url undefined 提前抛错，根本没进入采集，结果永远「尚未扫描」；而错误文案把「读不到 URL」伪装成了「页面非 http(s)」。

## 同链路次要问题

getCurrentReport 同样先判 tab.url undefined 就直接返回 report:null，即便 session 里有旧报告也读不出，放大「尚未扫描」观感。
main-world.js 用 scripting world:MAIN 注入有竞态，属已知 best-effort，不是本次主因。
扫描完成反馈靠 storage.onChanged，没有显式完成事件，url 读不到时整条链跑不起来。

## 修复建议（按优先级）

P0 让扫描真能读到 tab 并注入。推荐方案 A（贴合产品「按需授权」边界）：在 scanCurrentTab 拿到 tab 后，若 tab.url 读不到，用 chrome.permissions.request({ origins: [当前 origin /*] }) 请求该站点 host 权限再注入。注意 permissions.request 必须在用户手势中调用，稳妥做法是先加一个「为当前站点授权」按钮，在侧栏页里请求授予，granted 之后再发 scan_current_tab。

备选方案 B：直接在 permissions 里加 tabs，url 立刻可读；代价是 tabs 权限较重，与最小权限和商店披露冲突，不建议。

P1 改掉误导文案：读不到 url 时提示「无法读取当前页地址，请先为该站点授权或重新点击扩展图标」；真拿到 url 但非 http(s)（chrome、edge、file 等）时才提示「只扫描普通 http(s) 页面」。

P1 补一个显式的「为当前站点授权」入口，对齐已经声明的 optional_host_permissions，调用 chrome.permissions.request({ origins: [当前 origin /*] })，授予后仅对该 origin 生效，而非全量。

P2 让扫描完成反馈真实：content 脚本采集完毕后显式发 scan_completed（或在 verisilo_observation 上加阶段标志），侧栏据此显示「采集中/采集完成」，避免用户误以为卡住。

## 修复后验证

1. 干净 Edge 加载扩展，进入 https://dash.cloudflare.com/。
2. 点「为当前站点授权」→ 弹窗确认 → 返回 granted。
3. 点「扫描当前页面」不再出现 only scans regular HTTP(S) 报错。
4. 「当前结果」数秒内出现 navigator/canvas/webgl 等信号。
5. 在 edge 扩展详情确认 host 权限仅对 dash.cloudflare.com 生效，不是全量。
6. 切到 chrome settings 等页面扫描应得到「只扫描普通 http(s)」的（非误导）提示。

## 不在本次范围

「连接桌面端」未连接属预期（未注册白名单），与本 bug 无关。
桌面端 Silo 隔离、保险库等主线程能力正常，不受影响。
MAIN world 与 Worker 覆盖的 best-effort 边界，按已冻结的产品声明保留。
