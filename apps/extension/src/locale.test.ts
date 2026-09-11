import { describe, expect, it } from "vitest";

import { translateUiText } from "./locale.js";

describe("bilingual UI copy", () => {
  it("translates primary navigation and permission guidance", () => {
    expect(translateUiText("信号概览", "en")).toBe("Overview");
    expect(translateUiText("扫描当前页面", "en")).toBe("Scan current page");
    expect(translateUiText("关闭仅本机提示", "en")).toBe(
      "Dismiss local-only indicator",
    );
    expect(translateUiText("关闭提示", "en")).toBe("Dismiss message");
    expect(
      translateUiText(
        "浏览器尚未允许 VeriSilo 在隐私窗口中运行。请打开“扩展管理 → VeriSilo Companion”，允许在 Chrome 无痕或 Edge InPrivate 中运行，然后重试。",
        "en",
      ),
    ).toBe(
      "The browser has not allowed VeriSilo to run in private windows. Open Extension management → VeriSilo Companion, enable Allow in Incognito in Chrome or Allow in InPrivate in Edge, then try again.",
    );
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

  it("translates every segment of a composite device summary", () => {
    expect(
      translateUiText("1707 × 960 · 1.5× 缩放 · 16 GB 内存 · 4 线程", "en"),
    ).toBe("1707 × 960 · 1.5× scale · 16 GB memory · 4 threads");
  });

  it("translates background-originated error notices", () => {
    expect(
      translateUiText(
        "尚未获得当前页面的一次性访问权限。请关闭侧栏，在目标网页点击 VeriSilo 工具栏图标打开侧栏后再扫描。",
        "en",
      ),
    ).toBe(
      "The browser has not granted one-time access to the current page. Close the side panel, click the VeriSilo toolbar icon on the target page, then open the panel again before scanning.",
    );
    expect(translateUiText("VeriSilo 只扫描普通 HTTP(S) 页面。", "en")).toBe(
      "VeriSilo only scans ordinary HTTP(S) pages.",
    );
    expect(
      translateUiText(
        "无法访问当前页面。请在普通 HTTP(S) 页面点击 VeriSilo 工具栏图标后重新扫描；浏览器内部页面、商店页面和 PDF 不支持扫描。",
        "en",
      ),
    ).toContain(
      "browser-internal pages, store pages, and PDFs are not supported",
    );
    expect(
      translateUiText("只能为普通 HTTP(S) 页面请求站点访问权限。", "en"),
    ).toBe("Site access can only be requested for ordinary HTTP(S) pages.");
    expect(
      translateUiText(
        "此浏览器版本不支持逐站点访问请求。请从目标网页点击 VeriSilo 工具栏图标，以授予本页一次性扫描访问权限。",
        "en",
      ),
    ).toContain("per-site access requests");
    expect(
      translateUiText(
        "无法识别当前普通 HTTP(S) 站点，或当前站点没有可撤销的长期权限。",
        "en",
      ),
    ).toContain("no revocable persistent permission");
    expect(
      translateUiText(
        "尚未授权网络检查服务。VeriSilo 没有发送任何出口或 DNS 检查请求。",
        "en",
      ),
    ).toBe(
      "The network check services have not been authorized. VeriSilo has not sent any egress or DNS check request.",
    );
    expect(translateUiText("IP 出口：HTTP 403", "en")).toBe(
      "IP egress: HTTP 403",
    );
  });

  it("keeps Chinese copy unchanged when Chinese is selected", () => {
    expect(translateUiText("网络检查", "zh-CN")).toBe("网络检查");
  });
});
