# Pre-RC4 F-2 Accessibility Confirmation · QA Evidence

- 任务：对 pre-RC4 QA finding F-2 做只读确认——判定「部分原生按钮无法激活」到底是真实键盘
  accessibility bug，还是 WebView2/UI Automation harness boundary。
- 判定：**F-2 = NOT_REPRODUCED_AS_USER_ACCESSIBILITY_BUG / UI_AUTOMATION_PROVIDER_BOUNDARY**。
  真实键盘路径全部正常；不需要 UI fix task。不得据本报告修改产品。
- 候选：`origin/baseline/dev` = `d9bc4954c487afe845d60d5679e186a0f100344a`
  （branch `agent/qa/f2-kbd-access-confirm-ab4ea5`，task workspace vault
  `qa-f2-kbd-access-confirm-ab4ea5`，Vite port 15734）。
- 运行层级：Mode B Desktop Dev（`node scripts/dev-desktop.mjs core --port 15734 --vault …`）。
  理由：F-2 的判定对象是真实 Tauri/WebView2 窗口里的键盘与 UIA 行为，UI Preview（浏览器 mock）
  无法表达。未构建 installer，未进入 Mode C。

## 结论（供决策）

用户可用的两条激活路径——真实键盘（Tab 聚焦 + Enter/Space + dock 方向键）与真实鼠标点击
（OS 级 SendInput，非 UIA 合成）——在本环境对 F-2 点名的全部控件都正常，且产生真实产品状态变化。
程序化 UIA AXPress 在 identity-token / identity-focus 上"dispatched 但无效果"，与 F-2 原始记录一致；
同一树中普通 onClick 按钮（下一个身份）的 AXPress 正常生效。失败模式与控件暴露的 ARIA 状态
（`aria-pressed` / `aria-expanded`）相关，属于 WebView2 UIA provider / harness 合成激活的边界，
不是用户可感知的 keyboard accessibility bug。上一轮 QA 中"聚焦后 Enter/Space 无响应"的表述
未被复现——那部分观察来自与真实键盘不同的程序化路径。

**不需要 UI fix task。** 若后续 QA harness 需要程序化激活这些控件，应改用真实键鼠输入
（SendInput / 键盘语义），而不是把 AXPress 失败当作产品缺陷证据。

## 环境与方法

- 真实 Tauri Desktop（WebView2，dev build，非 installer）：窗口 pid 4784，`--vault qa-f2-kbd-access-confirm-ab4ea5`。
- 键盘输入：真实 OS 键盘事件（Tab / Shift+Tab / Enter / Space / ← / →）发送到前台窗口；
  焦点位置由 UIA 树的 focused 元素逐步确认。
- 鼠标输入：OS 级 SendInput（SetCursorPos + mouse_event down/up），坐标来自 UIA bounds
  （125% DPI 校准：PS 坐标 × 1.25 = 物理像素）。非 UIA 合成激活。
- 观测：UIA 元素树（名称 / 状态 / focused / bounds / 暴露的 patterns）。
- 测试数据：CLI 创建 2 个 Standard Silo（`qa-alpha` / `qa-beta`，Edge），
  vault 经真实键盘在 UI 内创建解锁。
- 上一轮 QA 的两个 no-op 陷阱均已规避：环境概览先切到其他 view 再回；identity token 先选中
  另一个 Silo 再激活目标 token。

## 逐对象结果

五组控件全部为原生 `<button type="button">` + onClick（`SiloList.tsx` / `shared/components.tsx`）。

### 1. 底部导航「环境概览」（TabButton，`aria-pressed`）

| 检查项 | 结果 |
| --- | --- |
| Tab 可达 | ✅（tabbar 内，Tab/Shift+Tab 双向到达） |
| Enter | ✅ 从「保险库与数据」view 切回 overview（身份场/dock 全部回归） |
| Space | ✅ 从「创建 Silo」view 切回 overview |
| 状态变化 | ✅ 两次均为真实 view 切换（先离开 overview，规避 no-op 陷阱） |
| 鼠标对照 | ✅ 点击「创建 Silo」→ 创建 view；再点击「环境概览」→ 回 overview |
| UIA AXPress | ⚠️ 未单独复测（同组 TabButton 本机工具见下；F-2 原记录称无响应） |

### 2. Identity dock「identity-token」（roving tabindex，`aria-pressed`）

| 检查项 | 结果 |
| --- | --- |
| Tab 可达 | ✅ 当前选中 token（tabIndex=0）；未选中 token 按 ARIA roving 模式设计上不可 Tab，由方向键到达 |
| Enter / Space | 语义 no-op：焦点 token 恒为已选中 token，choose(current) 无状态变化（预期行为，非激活失败） |
| 方向键激活 | ✅ ArrowLeft：qa-beta→qa-alpha（01/02、场景切换、roving 焦点移动）；ArrowRight 反向 ✅ |
| 状态变化 | ✅ 选中项、中央雕塑、上一/下一个按钮 disabled 状态全部联动 |
| 鼠标对照 | ✅ 点击未选中的 qa-alpha token → 选中并聚焦（02→01） |
| UIA AXPress | ❌ 复现 F-2：对未选中 qa-beta token AXPress "dispatched" 但无效果 |

### 3. 中央雕塑「identity-focus」（`aria-expanded`，带拖拽 pointer 处理）

| 检查项 | 结果 |
| --- | --- |
| Tab 可达 | ✅（场景内普通 tab 序） |
| Enter | ✅ identity lens 打开（QA-BETA 观测内容、收起证据按钮出现、焦点按 openLens 移入 lens 标题） |
| Space | ✅ lens 再次打开（关闭后焦点经 closeLens 回到触发按钮，Space 重开） |
| 状态变化 | ✅ lens 开/关 + `aria-expanded` + 焦点管理闭环；关闭用「收起证据」Enter 亦 ✅ |
| 鼠标对照 | ✅ 真实鼠标点击雕塑 → lens 打开（拖拽 pointer 逻辑不干扰普通点击，`event.detail===0` 判定正确放行键盘 click） |
| UIA AXPress | ❌ 复现 F-2：AXPress "dispatched" 但 lens 不打开 |

### 4. 对照：「本机工具」（TabButton，`aria-pressed`）

- Tab 可达 ✅；Enter ✅ 开 sheet；Space ✅ 开 sheet（两次独立验证）；「关闭本机工具」Enter 关闭 ✅，
  关闭后焦点回到触发按钮。sheet 内「检查浏览器」「更多操作」菜单（归档/编辑）键盘亦可用。
- 本机工具未做鼠标对照（上一轮 QA 中该按钮 AXToggle 已成功，此处键盘证据已充分）。

### 5. 对照：「上一个 / 下一个身份」（普通 onClick 按钮）

- 下一个身份：Enter ✅（qa-alpha→qa-beta，02/02，按钮 disabled 翻转）；Space ✅（qa-alpha→qa-beta）。
- 上一个身份：Enter ✅（qa-beta→qa-alpha，01/02）。
- 边界行为正确：首/尾 Silo 处相应按钮 disabled 且跳过 Tab，焦点掉落 body 后 Tab 从页首重走。
- UIA AXPress：✅ 对「下一个身份」正常生效（qa-alpha→qa-beta）——与 F-2 记录的"同树部分按钮正常"一致。

## UIA 边界探针（辅助证据，不构成判定依据）

| 目标 | 控件 ARIA 状态 | AXPress 结果 |
| --- | --- | --- |
| qa-beta identity-token | `aria-pressed` | dispatched，无效果（复现 F-2） |
| identity-focus | `aria-expanded` | dispatched，无效果（复现 F-2） |
| 下一个身份 | 无 | 正常生效 |
| （上一轮 QA）本机工具 | `aria-pressed` | AXToggle 正常生效——同 `aria-pressed` 组内行为仍不一致 |

失败面与 F-2 原始记录一致且仅在程序化合成激活路径上出现；真实键鼠输入不受影响。
对应实现事实：五个目标控件均为原生 `<button>` + onClick，Chromium 原生键盘语义
（Enter/Space → click）在 WebView2 内正常工作，产品侧无需 onKeyDown workaround。

## Focus ring

- 代码证据：`styles.css:83-88` 全局 `button:focus-visible { outline: 3px solid #1553ff; outline-offset: 3px }`
  覆盖全部目标控件；`spatial.css:189` 为 identity-focus 定义专属 dashed outline。
- 像素验证边界：本环境无屏幕捕获能力（`screenshot` unsupported，与上一轮 QA 相同），
  focus ring 的视觉呈现未能直接观测，以 UIA focused 元素逐跳确认键盘焦点位置代替。

## 边界与未覆盖

- 本环境无法截屏：focus ring、hover/动画等像素级结论不可得（同 F-3 边界）。
- AXPress 失败的 WebView2 provider 层根因（Invoke/Toggle/ExpandCollapse 到 DOM click 的桥接）
  未深入——超出本确认任务范围，且不影响用户路径判定。
- 托管（Camoufox）Silo 的 identity-token 未单独测试：dock/雕塑键盘语义与 engine 类型无关。
- 验证针对 dev build（Mode B）；WebView2 版本与已安装 runtime 一致，未做 installer 复测
  （与本任务判定无关，且未打开 RC gate）。
