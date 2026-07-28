import { describe, expect, it } from "vitest";

// Vite's test transform supplies the raw source; the desktop tsconfig does not
// include the broad `vite/client` ambient declarations on purpose.
// @ts-expect-error -- `?raw` is resolved by Vite/Vitest at test time.
import appSource from "./App.tsx?raw";

describe("desktop evidence copy", () => {
  it("keeps Companion inbox observations extension-asserted", () => {
    expect(appSource).toContain("Companion 声明的活动 Silo 观测");
    expect(appSource).toContain("<dt>当次请求出口</dt>");
    expect(appSource).toMatch(/证据级别保持\s+extension_asserted/u);
    expect(appSource).not.toContain("来自实际受管浏览器");
    expect(appSource).not.toContain("<dt>实际出口</dt>");
  });

  it("does not call the authenticated Provider receipt an independent proof", () => {
    expect(appSource).toContain("已认证的 Provider 删除回执");
    expect(appSource).toContain("不等同于第三方独立审计");
    expect(appSource).not.toContain("删除证明已验证并加密持久化");
  });
});
