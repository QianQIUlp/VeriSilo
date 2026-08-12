import { describe, expect, it } from "vitest";

import { translateUiText } from "./locale.js";

describe("bilingual UI copy", () => {
  it("translates primary navigation and permission guidance", () => {
    expect(translateUiText("信号概览", "en")).toBe("Overview");
    expect(translateUiText("扫描当前页面", "en")).toBe("Scan current page");
    expect(
      translateUiText(
        "先在目标网页点击工具栏中的 VeriSilo 图标。仍失败时，再请求该站点访问权限。",
        "en",
      ),
    ).toContain("VeriSilo toolbar icon");
  });

  it("translates dynamic counts and scopes without exposing state enums", () => {
    expect(translateUiText("3 条解读", "en")).toBe("3 findings");
    expect(
      translateUiText(
        "范围：example.com 的本机临时实验，不关联桌面身份，关闭浏览器或到期即失效。",
        "en",
      ),
    ).toContain("local temporary experiment for example.com");
    expect(translateUiText("启用时机：无法确认早于网站脚本", "en")).toBe(
      "Start timing: Cannot confirm it ran before site scripts",
    );
    expect(
      translateUiText(
        "覆盖：仅检查开启后新建的同站后台任务；同站内嵌页面检查通过；启用时机：无法确认早于网站脚本；网站数据：仅检查页面可见的随机测试标记；浏览器后台任务：仅检查后台任务的注册地址。",
        "en",
      ),
    ).not.toMatch(/[\u3400-\u9fff]/u);
    expect(
      translateUiText("实验室检查已因“页面已切换”自动停止并恢复。", "en"),
    ).toBe(
      "The Labs check stopped automatically because “Page changed” and restored the page.",
    );
    expect(
      translateUiText(
        "已确认 1/2 项；其余设置可能被策略或其他扩展接管。",
        "en",
      ),
    ).toContain("Confirmed 1/2 settings");
  });

  it("keeps Chinese copy unchanged when Chinese is selected", () => {
    expect(translateUiText("网络检查", "zh-CN")).toBe("网络检查");
  });
});
