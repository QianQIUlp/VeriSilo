# VeriSilo Companion Store RC 0.2.11 evidence

- 状态：**冻结证据页**
- 冻结日期：2026-08-15
- 用途：记录 Store Release Preparation 的唯一 RC 包、验证结果和已知边界。两个商店（Chrome Web Store、Edge Add-ons）只上传本页记录的同一份 ZIP。

## 冻结对象

| 项目                | 值                                                                                                      |
| ------------------- | ------------------------------------------------------------------------------------------------------- |
| 源码基线            | `main` @ `2f62e815700273f24e8cce429f251219e821efeb`（0.2.11 locale 修复）                               |
| 构建环境            | fresh git worktree + `pnpm install --frozen-lockfile`；pnpm 11.17.0，Node 24.19.0                       |
| `SOURCE_DATE_EPOCH` | `1786781541`（= `2f62e81` 的提交时间 2026-08-15T08:12:21Z）                                             |
| 归档文件名          | `VeriSilo-Companion-0.2.11-chrome-edge.zip`                                                             |
| 归档 SHA-256        | `3ab22f31eb5f37bcef9a37eb512ed273df90f4778ecc18b5c6d1c7e54408c76e`                                      |
| 归档字节数          | `1018149`                                                                                               |
| 文件数              | 21                                                                                                      |
| 格式                | ZIP32 stored，字节排序，DOS 时间戳来自 `SOURCE_DATE_EPOCH`                                              |
| Content manifest    | `artifacts/store-rc/extension-zip-manifest.json`（schema `urn:verisilo:deterministic-extension-zip:1`） |
| 制品位置            | `artifacts/store-rc/`（gitignored，与仓库其他证据制品同样不在 git 内）                                  |

0.2.11 相对 0.2.10 的唯一改动：把 background 硬编码中文错误文案加入双语词典（`ENGLISH_TEXT` 8 条 + `IP 出口` 动态模式），EN UI 不再出现中文错误 notice；版本号、bundle gate 版本与逐条配对检查同步更新。

## 已作废的旧哈希

| 来源           | 值                                                                 | 处理                     |
| -------------- | ------------------------------------------------------------------ | ------------------------ |
| 0.2.10 冻结 RC | `29e5485d9c7abb2c3da92b03a7ce97771d87f653a1354c269a1eb80ef6c5471b` | **作废**，被 0.2.11 取代 |
| PR #14 正文    | `01e18a35f641c928d92a98e46e94d643074ad54f128719e034021cabf2cbab7a` | **作废**，输入不可追溯   |
| 用户记录       | `74eeae8f72b80bc85fb91f4e43c163419d1b0a21653d8dae9f17b9f87805fbd3` | **作废**，输入不可追溯   |

发布与 Native Host 配置只认 0.2.11 的 SHA-256。

## 重建与验证记录

在 fresh worktree（`main@2f62e81`，冻结 lockfile）执行：

```text
pnpm check                                    -> 通过（extension / desktop / site，0 errors）
pnpm test                                     -> extension 12 files / 54 tests 通过；
                                                 desktop 12 files / 53 tests 通过；
                                                 session-fixture self-test 通过
pnpm extension:build                          -> 通过（icons 校验 + esbuild bundle）
pnpm extension:verify                         -> manifest / remote-code / disclosure /
                                                 双语错误文案配对 gate 通过
pnpm extension:package:self-test              -> 确定性 ZIP 自检通过

$env:SOURCE_DATE_EPOCH='1786781541'
node scripts/package-extension-zip.mjs --input apps/extension/dist \
  --out artifacts/store-rc/VeriSilo-Companion-0.2.11-chrome-edge.zip \
  --manifest artifacts/store-rc/extension-zip-manifest.json
node scripts/package-extension-zip.mjs --input apps/extension/dist \
  --out artifacts/store-rc/VeriSilo-Companion-0.2.11-chrome-edge.zip \
  --manifest artifacts/store-rc/extension-zip-manifest.json --check
```

`--check` 重新打包后与写入字节逐字节一致，证明同输入同 epoch 可重现同一归档。

## 旁加载冒烟（解压归档后加载，直接验证商店 ZIP 内容）

- 浏览器与版本：Edge Stable `151.0.4129.86`（`--load-extension`）；Chrome Stable `151.0.7922.139`（品牌版已移除 `--load-extension`，改用 CDP `Extensions.loadUnpacked`）。
- 方式：把归档解压到临时目录后加载，CDP 自动化驱动，独立临时 Profile（未触碰默认 Profile）。
- 结果：**两个浏览器各 27/27 checks passed**。

| 检查类别                   | 结果                                                                                                                                                                                                                                         |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| manifest                   | MV3、version 0.2.11、permissions 精确集合 `storage/sidePanel/activeTab/nativeMessaging/scripting`、optional `privacy`、optional host `http(s)://*/*`、无永久 host permissions、`side_panel.default_path=sidepanel.html`、`default_locale=en` |
| side panel UI              | EN 文案渲染；语言选择含 `en`+`zh-CN`；切到 `zh-CN` 渲染中文；tabs overview/isolation/labs/raw；scan 按钮存在；隔离面板存在                                                                                                                   |
| dismiss 行为               | notice 初始隐藏；local pill 初始可见（Local only）；两者均可关闭；重开 side panel 后 pill 恢复                                                                                                                                               |
| 无 activeTab 手势时的 scan | 优雅降级：显示可关闭的权限提示 notice，无崩溃                                                                                                                                                                                                |
| **EN 错误文案修复**        | scan 权限 notice 在 EN 语言下显示 "The browser has not granted one-time access to the current page…"，不再出现中文                                                                                                                           |
| service worker             | 存活；manifest 一致；`storage.local.setAccessLevel` API 可用（Chrome 的 SW 调试目标不暴露时改用消息往返验证）                                                                                                                                |
| console                    | side panel 页面无 error/warning                                                                                                                                                                                                              |

冒烟用临时 ID `egnejcalcdjkfhdcjfdpohdjanpcjibh`（unpacked 路径派生，**不是**任何商店 ID）。冒烟脚本与运行日志保存在 `artifacts/store-rc/`（`chrome-smoke.log` / `edge-smoke.log`）。

## 商店截图（已由账号持有人验收）

商店截图由 `scripts/capture-store-screenshots.mjs` 从解压后的 RC 重新捕获：左侧是明确标注的 `example.test` 示例网页构图，右侧是 RC 中实际加载的 Companion 面板；报告图使用合成的 `example.test` 脱敏夹具，不含真实用户数据。

最终素材为：

- `assets/store/store-screenshot-1280x800-en-scan.png`：英文扫描入口；
- `assets/store/store-screenshot-1280x800-zh-report.png`：简体中文扫描报告；
- `assets/store/store-screenshot-1280x800-en-private-space.png`：英文临时隐私空间；
- 对应的 `assets/store/store-screenshot-640x400-*.png`：同构图的 Chrome 兼容尺寸。

截图验收：六张文件分别为精确的 `1280×800` / `640×400`；已目检确认示例网页、面板标题、按钮、语言和报告内容可读，没有滚动条、空白错误态或被截断的卡片作为主视觉；隐私空间图只保留完整的首屏能力边界。素材不宣称反检测、受控指纹或网络隔离能力。

截图字节验收（SHA-256）：

| 文件                                             | 字节数 | SHA-256                                                            |
| ------------------------------------------------ | -----: | ------------------------------------------------------------------ |
| `store-screenshot-1280x800-en-scan.png`          | 296577 | `08a95877e3af143df35c9bd59268197d22de2fa405ef98346dae9b03c9176000` |
| `store-screenshot-1280x800-zh-report.png`        | 315382 | `52b12b595afb36087bbe68bd35c73d4047888b9ede75782c715a5179c40a9c2a` |
| `store-screenshot-1280x800-en-private-space.png` | 324554 | `e347a4f940cd6e0a94587bdd48854960d0e4ad1fa95e527320bd774eb71f9bce` |
| `store-screenshot-640x400-en-scan.png`           | 109469 | `690676df42f7b4876fdacc0fefb6d128caccc2d33ca707cef97e1beb91b667ff` |
| `store-screenshot-640x400-zh-report.png`         | 114054 | `0d7e22b5a50540be1b221bb5634909aeb36d89df729d409821c99bfead004685` |
| `store-screenshot-640x400-en-private-space.png`  | 120914 | `8befc804810e09c4a0f98bfbaf76302351be7d9bb44894af1af3030ffdb77b18` |

## 本机不可用项（不冒充已验证）

| 项                    | 状态与替代                                                                                             |
| --------------------- | ------------------------------------------------------------------------------------------------------ |
| Native Messaging 冒烟 | 本机无已构建的 `verisilo-native-host.exe` / 桌面端；`unavailable`。待 P6 拿到商店 ID 后按 runbook 验证 |

## 已知缺陷

0.2.10 记录的硬编码中文错误文案缺陷已在 0.2.11 修复并冒烟验证。当前无已知阻断缺陷。

## 你的人工补验清单（有桌面端的机器或 P6 阶段）

1. 在真实网页点击工具栏图标打开侧栏 → 执行扫描 → 确认报告生成（本机自动化无法模拟工具栏手势）。
2. 授权 optional host permission 后运行 Network Check，确认 ipwho.is / DoH 结果与本地降级路径。
3. 安装桌面端 + Native Host 后，确认 `Local only` 状态变为 Silo 绑定状态（P6 runbook）。

## 更新规则

任何代码、manifest、locale 或打包输入变化都产生新 SHA，必须重新走冻结流程并替换本页哈希；已作废哈希保留在表中供追溯。
