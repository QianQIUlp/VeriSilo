const PENDING_TOOLBAR_SCAN_KEY = "scan:pending-toolbar-authorization";
const DEFAULT_ACTION_TITLE = "Open VeriSilo Companion";

export const PENDING_TOOLBAR_SCAN_TTL_MS = 30_000;

interface PendingToolbarScan {
  tabId: number;
  requestedAt: number;
}

export async function armToolbarScan(
  tabId: number,
  requestedAt = Date.now(),
): Promise<void> {
  const previous = parsePendingToolbarScan(
    (await chrome.storage.session.get(PENDING_TOOLBAR_SCAN_KEY))[
      PENDING_TOOLBAR_SCAN_KEY
    ],
  );
  if (previous !== null && previous.tabId !== tabId) {
    await resetActionPrompt(previous.tabId);
  }

  await chrome.storage.session.set({
    [PENDING_TOOLBAR_SCAN_KEY]: { tabId, requestedAt },
  });
  await Promise.allSettled([
    chrome.action.setBadgeText({ tabId, text: "1" }),
    chrome.action.setBadgeBackgroundColor({ tabId, color: "#b3261e" }),
    chrome.action.setTitle({
      tabId,
      title: "点击授予当前标签页一次性访问并继续扫描",
    }),
  ]);
}

export async function consumeToolbarScan(
  tabId: number,
  now = Date.now(),
): Promise<boolean> {
  const pending = parsePendingToolbarScan(
    (await chrome.storage.session.get(PENDING_TOOLBAR_SCAN_KEY))[
      PENDING_TOOLBAR_SCAN_KEY
    ],
  );
  if (pending === null || pending.tabId !== tabId) {
    return false;
  }

  await chrome.storage.session.remove(PENDING_TOOLBAR_SCAN_KEY);
  await resetActionPrompt(tabId);
  return (
    now >= pending.requestedAt &&
    now - pending.requestedAt <= PENDING_TOOLBAR_SCAN_TTL_MS
  );
}

export async function clearToolbarScanForTab(tabId: number): Promise<void> {
  const pending = parsePendingToolbarScan(
    (await chrome.storage.session.get(PENDING_TOOLBAR_SCAN_KEY))[
      PENDING_TOOLBAR_SCAN_KEY
    ],
  );
  if (pending?.tabId !== tabId) {
    return;
  }
  await chrome.storage.session.remove(PENDING_TOOLBAR_SCAN_KEY);
  await resetActionPrompt(tabId);
}

function parsePendingToolbarScan(value: unknown): PendingToolbarScan | null {
  if (
    typeof value !== "object" ||
    value === null ||
    !("tabId" in value) ||
    !("requestedAt" in value) ||
    typeof value.tabId !== "number" ||
    !Number.isSafeInteger(value.tabId) ||
    value.tabId < 0 ||
    typeof value.requestedAt !== "number" ||
    !Number.isSafeInteger(value.requestedAt) ||
    value.requestedAt < 0
  ) {
    return null;
  }
  return { tabId: value.tabId, requestedAt: value.requestedAt };
}

async function resetActionPrompt(tabId: number): Promise<void> {
  await Promise.allSettled([
    chrome.action.setBadgeText({ tabId, text: "" }),
    chrome.action.setTitle({ tabId, title: DEFAULT_ACTION_TITLE }),
  ]);
}
