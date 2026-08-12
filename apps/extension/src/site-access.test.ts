import { afterEach, describe, expect, it, vi } from "vitest";

import { requestSiteAccessForTab } from "./site-access.js";

afterEach(() => {
  vi.unstubAllGlobals();
});

function installChromeMock(options?: {
  contains?: boolean;
  addError?: Error;
  supportsRequest?: boolean;
}) {
  const contains = vi.fn(async () => options?.contains ?? false);
  const addHostAccessRequest = vi.fn(async () => {
    if (options?.addError !== undefined) {
      throw options.addError;
    }
  });
  vi.stubGlobal("chrome", {
    permissions: {
      contains,
      ...(options?.supportsRequest === false ? {} : { addHostAccessRequest }),
    },
  });
  return { contains, addHostAccessRequest };
}

describe("current-tab site access", () => {
  it("raises a browser-owned host access request when navigation hid the URL", async () => {
    const chromeMock = installChromeMock();

    await expect(requestSiteAccessForTab({ id: 42 })).resolves.toEqual({
      requested: true,
      alreadyGranted: false,
    });
    expect(chromeMock.addHostAccessRequest).toHaveBeenCalledWith({ tabId: 42 });
    expect(chromeMock.contains).not.toHaveBeenCalled();
  });

  it("reuses an explicit site grant without adding another request", async () => {
    const chromeMock = installChromeMock({ contains: true });

    await expect(
      requestSiteAccessForTab({ id: 7, url: "https://example.test/path" }),
    ).resolves.toEqual({ requested: false, alreadyGranted: true });
    expect(chromeMock.contains).toHaveBeenCalledWith({
      origins: ["https://example.test/*"],
    });
    expect(chromeMock.addHostAccessRequest).not.toHaveBeenCalled();
  });

  it("recognizes an existing one-time grant without requesting broad access", async () => {
    installChromeMock({ addError: new Error("Tab already has access") });

    await expect(requestSiteAccessForTab({ id: 9 })).resolves.toEqual({
      requested: false,
      alreadyGranted: false,
      temporaryAccess: true,
    });
  });

  it("does not request access for browser-internal pages", async () => {
    const chromeMock = installChromeMock();

    await expect(
      requestSiteAccessForTab({ id: 3, url: "edge://extensions" }),
    ).rejects.toThrow("普通 HTTP(S) 页面");
    expect(chromeMock.addHostAccessRequest).not.toHaveBeenCalled();
  });
});
