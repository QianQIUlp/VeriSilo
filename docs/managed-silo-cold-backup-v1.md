# Managed Silo 冷备份与原身份恢复 v1

本轮目标：补齐长期身份资产的恢复链，让一个已停止的本地 Managed Silo 连同浏览器持久数据、原始身份制品、引擎绑定和必要网络配置一起恢复，且不影响另一个 Silo。起点为 `7b0a65dfd444582d6f9459f695f0417ab209480b`；实现与验收状态以本轮 QA 记录为准。

## 范围与决策

- 同一台 Windows、同一系统用户、同一 Vault 路径，恢复现有同 UUID/种子的原身份。Profile 丢失可以恢复；Silo 元数据也被删除时，先恢复已有 Vault 配置备份。
- 冷备份要求浏览器停止且 Profile 占用解除。独立备份密码保护归档，Windows 用户保护和身份绑定限制恢复范围；Vault 口令轮换不应使冷备份失效。
- 完整范围是 Profile 持久文件、原始 Artifact、身份/引擎绑定和必要配置/网络秘密；不打包引擎可执行文件，也不保存可复用的当前运行证据。
- 恢复前检查备份并显示覆盖范围；确认绑定到检查所得归档摘要。后端重新校验，安全暂存后替换，普通失败和进程中断都必须保住原数据或留下可恢复的明确状态。
- 缺失网络秘密、不兼容引擎、错误密码、损坏输入、活动 Profile、路径越界和身份冲突必须在破坏原数据之前拒绝。不能偷偷直连。
- 历史运行记录仍为历史；恢复后必须重新启动取得当前证据。服务器撤销的登录状态不在恢复保证之内。

现有 Vault 备份仍只备份配置。新功能增强 Identity Integrity、Execution Integrity、Network Integrity 与 Attribution，不扩大指纹声明或声称更强反检测效果。

Managed 运行实际使用 `silos/<UUID>/profiles/`，不能仅复制 Silo 元数据中的 `browser-data` 占位路径。运行态 `engine-state` 与可重新物化的 `identity` 缓存不应恢复为当前状态；原始 Artifact 来自加密 Vault 记录。

Windows 用户与机器限制必须同时成立。[Microsoft DPAPI 文档](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)指出漫游用户配置可能跨机器解密 CurrentUser 数据，因此归档密钥使用用户保护与机器保护两层包装；单独使用机器保护也不能替代用户限制。

## 验收与停止条件

在独立开发 Vault 中创建 A/B，向本地测试站写入不同 Cookie、LocalStorage 和 IndexedDB。停止 A 后备份，改变其数据，再恢复并真实启动读回备份时的状态；确认 UUID、原始 Artifact 和绑定保持，B 不受影响。用现有 focused tests 覆盖错误密码、损坏备份、活动占用、不兼容输入及失败回滚。UI Preview 检查选择、检查摘要、确认覆盖、错误和秘密清理，不代替 runtime evidence。

共享 DTO 变化按 integration 工作流执行完整自动化矩阵。真实运行使用现有固定引擎和 CLI/真实 backend；不重建内核、不做 RC、installer、生产打包或发布验收。开发 branch/baseline 发布与产品 release 分开。

完成以恢复用户动作和必要失败保护得到直接证据为准，不能仅凭提交、测试通过或构建完成关闭目标。遇到缺少必要访问/授权时保留未完成项；工具故障不无限接管任务。

## 本轮执行约束

1. 每项工作对应具体产品问题或必要不确定性，直接继承有效成果与证据。
2. 每轮修复跟随最低充分验证，优先现有工具。
3. 没有新证据或新假设，不重复昂贵构建、安装和验收。
4. 历史 Sandbox 不是默认续接点，不自动开启 RC 或生产发布流程。
5. 必要产品工作未完成时保持目标开放；有限并发与后续维护阶段不自动纳入本轮。
