import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { previewManagedIdentity, previewManagedSilo } from "../../preview/fixtures.js";

import { CreatedSiloSummary } from "./CreatedSiloSummary.js";

const newlyCreatedSilo = {
  ...previewManagedSilo,
  name: "新建托管空间",
  identityLockedAt: null,
};

const renderSummary = (
  preview: typeof previewManagedIdentity | undefined,
  state: {
    busy?: boolean;
    launching?: boolean;
    launchBlocked?: boolean;
    launchBlockedReason?: string;
  } = {},
) =>
  renderToStaticMarkup(
    createElement(CreatedSiloSummary, {
      silo: newlyCreatedSilo,
      preview,
      onInspect: () => {},
      onLaunch: () => {},
      onDismiss: () => {},
      ...state,
    }),
  );

describe("CreatedSiloSummary", () => {
  it("shows the resolved Managed identity and keeps network values declared", () => {
    const markup = renderSummary(previewManagedIdentity);
    expect(markup).toContain("新建托管空间");
    expect(markup).toContain("en-US");
    expect(markup).toContain("America/New_York");
    expect(markup).toContain("1920×1080");
    expect(markup).toContain("Camoufox · 独立 Firefox");
    expect(markup).toContain("http://proxy.example.test:3128");
    expect(markup).toContain("网络位置配置");
    expect(markup).toContain("已关联身份配置");
    expect(markup).toContain("预期出口地址");
    expect(markup).toContain("203.0.113.7");
    expect(markup).toContain("独立保存 Cookie、登录状态和网站数据");
    expect(markup).toContain("首次成功启动后锁定");
    expect(markup).toContain("并非浏览器运行观察");
    expect(markup).toContain('aria-labelledby="created-silo-summary-title"');
    expect(markup).toContain("查看配置");
    expect(markup).toContain("打开浏览器");
    expect(markup).toContain("关闭摘要");
  });

  it("does not invent resolved values while a preview is missing", () => {
    const markup = renderSummary(undefined);
    expect(markup).toContain("解析配置暂不可用");
    expect(markup).toContain("加载尚未完成或读取失败");
    expect(markup).toContain("查看配置");
    expect(markup).toContain("打开浏览器");
    expect(markup).not.toContain("en-US");
    expect(markup).not.toContain("America/New_York");
    expect(markup).not.toContain("已关联身份配置");
    expect(markup).not.toContain("预期出口地址");
  });

  it("distinguishes an unmarked direct artifact and an already locked identity", () => {
    const markup = renderToStaticMarkup(
      createElement(CreatedSiloSummary, {
        silo: {
          ...newlyCreatedSilo,
          identityLockedAt: "2026-09-28T00:00:00Z",
          networkProfile: { mode: "direct", proxyRequired: false },
        },
        preview: {
          ...previewManagedIdentity,
          countryCode: null,
          publicAddress: null,
          networkBound: false,
        },
        onInspect: () => {},
        onLaunch: () => {},
        onDismiss: () => {},
      }),
    );
    expect(markup).toContain("直连，不走代理");
    expect(markup).toContain("未标记关联");
    expect(markup).toContain("浏览器身份配置与运行位置已锁定");
    expect(markup).not.toContain("预期出口地址");
  });

  it("keeps inspection and dismissal available while another operation is busy", () => {
    const markup = renderSummary(previewManagedIdentity, { busy: true });
    expect(markup.match(/<button[^>]*disabled=""/g)).toHaveLength(1);
    expect(markup).toContain("正在处理其他操作");
    expect(markup).not.toContain("正在打开…");
  });

  it("shows opening only for this Silo and explains a blocked launch", () => {
    const launching = renderSummary(previewManagedIdentity, {
      launching: true,
      launchBlocked: true,
    });
    expect(launching.match(/<button[^>]*disabled=""/g)).toHaveLength(1);
    expect(launching).toContain("正在打开…");
    expect(launching).not.toContain("已有 Silo 正在运行");

    const blocked = renderSummary(previewManagedIdentity, {
      launchBlocked: true,
      launchBlockedReason: "请先停止正在运行的 Silo。",
    });
    expect(blocked.match(/<button[^>]*disabled=""/g)).toHaveLength(1);
    expect(blocked).toContain("请先停止正在运行的 Silo。");
    expect(blocked).toContain('aria-describedby="created-silo-launch-hint"');
    expect(blocked).not.toContain("正在打开…");
  });
});
