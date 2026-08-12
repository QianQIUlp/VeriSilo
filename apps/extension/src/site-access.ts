export interface SiteAccessRequestResult {
  requested: boolean;
  alreadyGranted: boolean;
  temporaryAccess?: boolean;
}

export async function requestSiteAccessForTab(
  tab: Pick<chrome.tabs.Tab, "id" | "url">,
): Promise<SiteAccessRequestResult> {
  if (tab.id === undefined) {
    throw new Error("No active browser tab is available.");
  }
  if (tab.url !== undefined && !/^https?:/u.test(tab.url)) {
    throw new Error("只能为普通 HTTP(S) 页面请求站点访问权限。");
  }

  const originPattern =
    tab.url === undefined ? null : `${new URL(tab.url).origin}/*`;
  if (
    originPattern !== null &&
    (await chrome.permissions.contains({ origins: [originPattern] }))
  ) {
    return { requested: false, alreadyGranted: true };
  }

  type HostAccessRequestApi = typeof chrome.permissions & {
    addHostAccessRequest?: (request: { tabId: number }) => Promise<void>;
  };
  const permissions = chrome.permissions as HostAccessRequestApi;
  if (permissions.addHostAccessRequest === undefined) {
    throw new Error(
      "此浏览器版本不支持逐站点访问请求。请从目标网页点击 VeriSilo 工具栏图标，以授予本页一次性扫描访问权限。",
    );
  }

  try {
    await permissions.addHostAccessRequest({ tabId: tab.id });
    return { requested: true, alreadyGranted: false };
  } catch (error) {
    if (/already has access|已有.*访问|已经.*访问/iu.test(errorMessage(error))) {
      const alreadyGranted =
        originPattern !== null &&
        (await chrome.permissions.contains({ origins: [originPattern] }));
      return {
        requested: false,
        alreadyGranted,
        temporaryAccess: !alreadyGranted,
      };
    }
    throw error;
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
