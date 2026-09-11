import { spawn, spawnSync } from "node:child_process";
import http from "node:http";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";

const edgePath =
  process.env.VERISILO_EDGE_PATH ??
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";
const extensionDirectory = path.resolve(process.argv[2] ?? "");
const outputDirectory = path.resolve(process.argv[3] ?? "");
const port = Number(process.env.VERISILO_SCREENSHOT_PORT ?? 9347);

if (!process.argv[2] || !process.argv[3]) {
  throw new Error(
    "Usage: node scripts/capture-store-screenshots.mjs <extension-dir> <output-dir>",
  );
}

const collectedAt = "2026-08-15T00:00:00.000Z";
const report = {
  schemaVersion: 1,
  reportId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
  origin: "https://example.test",
  collectedAt,
  coverage: { mainWorld: "observed", worker: "self_test_only" },
  signals: [
    {
      id: "navigator",
      source: "window",
      status: "ok",
      stability: "stable",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: {
        userAgent:
          "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/151.0.0.0 Safari/537.36 Edg/151.0.0.0",
        platform: "Win32",
        language: "zh-CN",
        languages: ["zh-CN", "en-US"],
        deviceMemory: 16,
        hardwareConcurrency: 16,
        maxTouchPoints: 0,
      },
    },
    {
      id: "timezone",
      source: "window",
      status: "ok",
      stability: "stable",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: "Asia/Singapore",
    },
    {
      id: "ua_ch",
      source: "window",
      status: "ok",
      stability: "stable",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: { highEntropy: { bitness: "64" } },
    },
    {
      id: "screen",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: { width: 1920, height: 1080, pixelRatio: 1 },
    },
    {
      id: "webgl",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "high",
      collectedAt,
      durationMs: 1,
      value: {
        renderer:
          "ANGLE (NVIDIA, NVIDIA GeForce RTX 4060 Laptop GPU (0x000028E0) Direct3D11)",
      },
    },
    {
      id: "canvas_hash",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "high",
      collectedAt,
      durationMs: 1,
      value: { digest: "demo-canvas-digest" },
    },
    {
      id: "audio",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "high",
      collectedAt,
      durationMs: 1,
      value: { digest: "demo-audio-digest" },
    },
    {
      id: "fonts",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "high",
      collectedAt,
      durationMs: 1,
      value: { digest: "demo-font-digest", count: 18 },
    },
    {
      id: "storage",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: { cookiesEnabled: true, localStorage: true, indexedDb: true },
    },
    {
      id: "permissions",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: { geolocation: "denied", notifications: "default" },
    },
  ],
};

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function requestJson(requestPath, method = "GET") {
  const response = await new Promise((resolve, reject) => {
    const request = http.request(
      { host: "127.0.0.1", port, path: requestPath, method },
      (result) => {
        let body = "";
        result.on("data", (chunk) => (body += chunk));
        result.on("end", () =>
          resolve({ status: result.statusCode ?? 0, body }),
        );
      },
    );
    request.on("error", reject);
    request.end();
  });
  if (response.status !== 200) {
    throw new Error(
      `HTTP ${response.status} for ${requestPath}: ${response.body}`,
    );
  }
  return JSON.parse(response.body);
}

function connectCdp(webSocketUrl) {
  const socket = new WebSocket(webSocketUrl);
  let nextId = 0;
  const pending = new Map();
  const listeners = new Map();
  const ready = new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve);
    socket.addEventListener("error", reject);
  });

  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id !== undefined) {
      const request = pending.get(message.id);
      if (request === undefined) return;
      pending.delete(message.id);
      if (message.error !== undefined) {
        request.reject(new Error(message.error.message));
      } else {
        request.resolve(message.result);
      }
      return;
    }
    if (message.method !== undefined) {
      for (const listener of listeners.get(message.method) ?? []) {
        listener(message.params);
      }
    }
  });

  return {
    ready,
    send(method, params = {}) {
      return new Promise((resolve, reject) => {
        const id = ++nextId;
        pending.set(id, { resolve, reject });
        socket.send(JSON.stringify({ id, method, params }));
      });
    },
    on(method, listener) {
      listeners.set(method, [...(listeners.get(method) ?? []), listener]);
    },
    close() {
      socket.close();
    },
  };
}

async function evaluate(client, expression, userGesture = false) {
  const result = await client.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    userGesture,
  });
  if (result.exceptionDetails !== undefined) {
    throw new Error(result.exceptionDetails.text);
  }
  return result.result.value;
}

async function openTarget(url) {
  return requestJson(`/json/new?${encodeURIComponent(url)}`, "PUT");
}

async function waitForServiceWorker() {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const targets = await requestJson("/json/list");
      const target = targets.find((candidate) =>
        /^chrome-extension:\/\/[a-p]{32}\/background\.js$/u.test(candidate.url),
      );
      if (target !== undefined) return target;
    } catch {
      // Edge is still starting.
    }
    await delay(500);
  }
  throw new Error("The extension service worker did not start.");
}

async function writeScreenshot(client, filePath, clip = undefined) {
  const result = await client.send("Page.captureScreenshot", {
    format: "png",
    ...(clip === undefined ? {} : { clip }),
  });
  await writeFile(filePath, Buffer.from(result.data, "base64"));
}

async function hideScrollbars(client) {
  await evaluate(
    client,
    `(() => {
      if (document.getElementById("verisilo-store-capture-style")) return;
      const style = document.createElement("style");
      style.id = "verisilo-store-capture-style";
      style.textContent = "::-webkit-scrollbar{display:none!important}html{scrollbar-width:none}";
      document.head.append(style);
    })()`,
  );
}

async function capturePanel(client, filePath, width = 420, height = 672) {
  await client.send("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await hideScrollbars(client);
  await delay(500);
  await writeScreenshot(client, filePath);
}

function escapeHtml(value) {
  return value.replace(
    /[&<>"']/gu,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[character],
  );
}

function showcaseHtml({ language, panelBase64, scene }) {
  const chinese = language === "zh";
  const page = chinese
    ? {
        kicker: "示例网页 · HTTP(S)",
        title: "先看懂当前网页能看到什么",
        body: "VeriSilo Companion 把页面可见的浏览器信号整理成易读的本地报告。扫描由你触发，不会自动收集。",
        chips: ["语言与时区", "设备与屏幕", "图形与字体"],
        note: "本地观察 · 不上传扫描报告",
      }
    : scene === "private"
      ? {
          kicker: "EXAMPLE PAGE · BROWSER CONTEXT",
          title: "Temporary controls, clearly bounded.",
          body: "Review the browser-provided private workspace and temporary controls without presenting them as a permanent identity container.",
          chips: ["Browser-provided", "Temporary", "Restorable"],
          note: "State is explicit · Nothing runs automatically",
        }
      : {
          kicker: "EXAMPLE PAGE · HTTP(S)",
          title: "Make browser signals legible.",
          body: "VeriSilo Companion turns page-visible browser signals into a local, readable report so you can understand the current environment before acting.",
          chips: ["Language & timezone", "Device & screen", "Graphics & fonts"],
          note: "On-demand inspection · No automatic collection",
        };
  const panelCaption = chinese
    ? "实际 Companion 界面 · 本地观察"
    : scene === "private"
      ? "Companion · Temporary private space"
      : "Companion · On-demand inspection";

  return `<!doctype html>
<html lang="${chinese ? "zh-CN" : "en"}">
  <head>
    <meta charset="UTF-8" />
    <style>
      :root { color-scheme: light; font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif; color: #172036; }
      * { box-sizing: border-box; }
      html, body { width: 1280px; height: 800px; margin: 0; overflow: hidden; }
      body { background: radial-gradient(circle at 10% 0%, #e5e4ff 0, transparent 28%), linear-gradient(135deg, #eef0f9, #f8f9fc 58%, #e6e8f8); }
      .canvas { width: 1280px; height: 800px; padding: 34px 44px; }
      .browser { width: 1192px; height: 732px; overflow: hidden; border: 1px solid #dce1ef; border-radius: 24px; background: #fff; box-shadow: 0 24px 60px rgba(36, 43, 85, .18), 0 4px 12px rgba(36, 43, 85, .08); }
      .browser-bar { display: flex; align-items: center; gap: 14px; height: 58px; padding: 0 20px; border-bottom: 1px solid #e6e9f2; background: #f6f7fb; }
      .dots { display: flex; gap: 7px; }
      .dots i { width: 10px; height: 10px; border-radius: 50%; background: #d7dbe7; }
      .dots i:first-child { background: #ffb8a4; }
      .dots i:nth-child(2) { background: #f4d181; }
      .dots i:nth-child(3) { background: #9dddbd; }
      .address { display: flex; align-items: center; gap: 9px; width: 520px; height: 32px; padding: 0 13px; border: 1px solid #e0e4ef; border-radius: 9px; color: #65708a; background: #fff; font: 12px ui-monospace, SFMono-Regular, Consolas, monospace; }
      .address b { color: #12a66b; font-size: 12px; }
      .browser-label { margin-left: auto; color: #8b93a7; font-size: 11px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
      .workspace { display: grid; grid-template-columns: 1fr 420px; height: 674px; background: #f5f6fb; }
      .page { padding: 46px 42px; background: linear-gradient(155deg, #f9faff, #edf0f9); }
      .page-kicker { color: #5b5ce2; font-size: 11px; font-weight: 800; letter-spacing: .14em; }
      h1 { max-width: 520px; margin: 18px 0 16px; font-size: 43px; line-height: 1.04; letter-spacing: -.045em; }
      .lead { max-width: 530px; margin: 0; color: #58647c; font-size: 16px; line-height: 1.6; }
      .signal-card { margin-top: 34px; padding: 22px; border: 1px solid #e0e4f0; border-radius: 18px; background: rgba(255,255,255,.82); box-shadow: 0 10px 24px rgba(35, 44, 92, .06); }
      .signal-card-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 18px; }
      .signal-card-head strong { font-size: 14px; }
      .signal-card-head span { color: #667085; font-size: 11px; }
      .chips { display: flex; flex-wrap: wrap; gap: 8px; }
      .chip { padding: 8px 10px; border: 1px solid #dce1f2; border-radius: 999px; color: #4248a8; background: #f0f0ff; font-size: 11px; font-weight: 700; }
      .note { display: flex; align-items: center; gap: 9px; margin-top: 26px; color: #667085; font-size: 12px; }
      .note i { width: 8px; height: 8px; border-radius: 50%; background: #12b76a; box-shadow: 0 0 0 4px #dff7eb; }
      .panel-wrap { display: flex; align-items: flex-start; justify-content: center; padding: 0; overflow: hidden; border-left: 1px solid #e1e5f0; background: #f4f6fb; }
      .panel-wrap img { display: block; width: 420px; height: 672px; object-fit: cover; object-position: top; }
      .panel-label { position: absolute; right: 466px; bottom: 49px; padding: 6px 9px; border: 1px solid rgba(255,255,255,.78); border-radius: 7px; color: #667085; background: rgba(255,255,255,.88); font-size: 10px; font-weight: 700; }
    </style>
  </head>
  <body>
    <main class="canvas">
      <section class="browser" aria-label="VeriSilo Companion store preview">
        <header class="browser-bar">
          <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
          <span class="address"><b>●</b> ${chinese ? "example.test/检查页" : "example.test/inspection"}</span>
          <span class="browser-label">${escapeHtml(panelCaption)}</span>
        </header>
        <div class="workspace">
          <section class="page">
            <div class="page-kicker">${escapeHtml(page.kicker)}</div>
            <h1>${escapeHtml(page.title)}</h1>
            <p class="lead">${escapeHtml(page.body)}</p>
            <div class="signal-card">
              <div class="signal-card-head"><strong>${chinese ? "页面可见信号" : "Page-visible signals"}</strong><span>${chinese ? "按需检查" : "On demand"}</span></div>
              <div class="chips">${page.chips.map((chip) => `<span class="chip">${escapeHtml(chip)}</span>`).join("")}</div>
            </div>
            <p class="note"><i></i>${escapeHtml(page.note)}</p>
          </section>
          <aside class="panel-wrap"><img src="data:image/png;base64,${panelBase64}" alt="VeriSilo Companion panel" /></aside>
        </div>
      </section>
    </main>
  </body>
</html>`;
}

async function renderShowcase(client, html, output1280, output640) {
  await client.send("Emulation.setDeviceMetricsOverride", {
    width: 1280,
    height: 800,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await evaluate(
    client,
    `(() => {
      document.open();
      document.write(${JSON.stringify(html)});
      document.close();
    })()`,
  );
  await evaluate(
    client,
    `Promise.all([...document.images].map((image) => image.complete ? Promise.resolve() : new Promise((resolve) => { image.addEventListener("load", resolve, { once: true }); image.addEventListener("error", resolve, { once: true }); })))`,
  );
  await delay(500);
  await writeScreenshot(client, output1280, {
    x: 0,
    y: 0,
    width: 1280,
    height: 800,
    scale: 1,
  });
  await writeScreenshot(client, output640, {
    x: 0,
    y: 0,
    width: 1280,
    height: 800,
    scale: 0.5,
  });
}

const temporaryRoot = await mkdtemp(
  path.join(tmpdir(), "verisilo-store-capture-"),
);
const profileDirectory = path.join(temporaryRoot, "profile");
const panelDirectory = path.join(temporaryRoot, "panel");
await mkdir(panelDirectory, { recursive: true });
await mkdir(outputDirectory, { recursive: true });

let edge;
let serviceWorkerClient;
let panelClient;
let showcaseClient;

try {
  edge = spawn(
    edgePath,
    [
      `--user-data-dir=${profileDirectory}`,
      `--disable-extensions-except=${extensionDirectory}`,
      `--load-extension=${extensionDirectory}`,
      `--remote-debugging-port=${port}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--no-session-restore",
      "--window-size=1280,800",
      "about:blank",
    ],
    { stdio: "ignore", detached: true },
  );

  const serviceWorkerTarget = await waitForServiceWorker();
  const extensionId = new URL(serviceWorkerTarget.url).host;
  serviceWorkerClient = connectCdp(serviceWorkerTarget.webSocketDebuggerUrl);
  await serviceWorkerClient.ready;

  const panelTarget = await openTarget(
    `chrome-extension://${extensionId}/sidepanel.html`,
  );
  panelClient = connectCdp(panelTarget.webSocketDebuggerUrl);
  await panelClient.ready;
  await panelClient.send("Runtime.enable");
  await panelClient.send("Page.enable");
  await delay(2_000);

  const panelEnPath = path.join(panelDirectory, "panel-en-ready.png");
  await evaluate(
    panelClient,
    `(() => {
      const select = document.getElementById("language-select");
      if (select !== null && select.value !== "en") {
        select.value = "en";
        select.dispatchEvent(new Event("change"));
      }
    })()`,
  );
  await delay(1_000);
  await capturePanel(panelClient, panelEnPath);

  const reportLiteral = JSON.stringify(report);
  await panelClient.send("Page.reload", { ignoreCache: true });
  await delay(2_000);
  await evaluate(
    panelClient,
    `(async () => {
      const current = await chrome.tabs.getCurrent();
      const active = (await chrome.tabs.query({ active: true, lastFocusedWindow: true }))[0];
      const ids = [...new Set([current?.id, active?.id].filter((id) => id !== undefined))];
      if (ids.length === 0) throw new Error("Unable to identify the capture tab.");
      await chrome.storage.session.set(
        Object.fromEntries(ids.map((id) => ["report:" + id, ${reportLiteral}])),
      );
      return ids;
    })()`,
  );
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const hasReport = await evaluate(
      panelClient,
      `document.getElementById("report-content")?.hidden === false`,
    );
    if (hasReport === true) break;
    await delay(250);
    if (attempt === 19) {
      throw new Error(
        "The seeded report did not render in the Companion panel.",
      );
    }
  }
  await evaluate(
    panelClient,
    `(() => {
      const select = document.getElementById("language-select");
      if (select !== null) {
        select.value = "zh-CN";
        select.dispatchEvent(new Event("change"));
      }
    })()`,
  );
  await delay(1_500);
  await evaluate(
    panelClient,
    `(() => {
      document.body.style.transform = "scale(0.84)";
      document.body.style.transformOrigin = "top center";
      document.querySelectorAll("#fact-grid > details").forEach((element, index) => {
        if (index >= 3) element.remove();
      });
      document.querySelector('#report-content > article:nth-of-type(3)')?.remove();
      document.querySelector("#report-content > .scope-note")?.remove();
    })()`,
  );
  await delay(300);
  const panelZhPath = path.join(panelDirectory, "panel-zh-report.png");
  await capturePanel(panelClient, panelZhPath);

  await evaluate(
    panelClient,
    `(() => {
      document.body.style.transform = "";
      document.body.style.transformOrigin = "";
      const select = document.getElementById("language-select");
      if (select !== null) {
        select.value = "en";
        select.dispatchEvent(new Event("change"));
      }
      document.querySelector('[data-tab="isolation"]')?.click();
      document.querySelector(".capability-stack")?.remove();
      document.querySelector(".desktop-callout")?.remove();
    })()`,
  );
  await delay(1_500);
  const panelPrivatePath = path.join(panelDirectory, "panel-en-private.png");
  await capturePanel(panelClient, panelPrivatePath);

  const panelEnBase64 = (await readFile(panelEnPath)).toString("base64");
  const panelZhBase64 = (await readFile(panelZhPath)).toString("base64");
  const panelPrivateBase64 = (await readFile(panelPrivatePath)).toString(
    "base64",
  );

  const showcaseTarget = await openTarget("about:blank");
  showcaseClient = connectCdp(showcaseTarget.webSocketDebuggerUrl);
  await showcaseClient.ready;
  await showcaseClient.send("Runtime.enable");
  await showcaseClient.send("Page.enable");

  await renderShowcase(
    showcaseClient,
    showcaseHtml({ language: "en", panelBase64: panelEnBase64, scene: "scan" }),
    path.join(outputDirectory, "store-screenshot-1280x800-en-scan.png"),
    path.join(outputDirectory, "store-screenshot-640x400-en-scan.png"),
  );
  await renderShowcase(
    showcaseClient,
    showcaseHtml({
      language: "zh",
      panelBase64: panelZhBase64,
      scene: "report",
    }),
    path.join(outputDirectory, "store-screenshot-1280x800-zh-report.png"),
    path.join(outputDirectory, "store-screenshot-640x400-zh-report.png"),
  );
  await renderShowcase(
    showcaseClient,
    showcaseHtml({
      language: "en",
      panelBase64: panelPrivateBase64,
      scene: "private",
    }),
    path.join(
      outputDirectory,
      "store-screenshot-1280x800-en-private-space.png",
    ),
    path.join(outputDirectory, "store-screenshot-640x400-en-private-space.png"),
  );

  console.log(`Captured store screenshots from extension ${extensionId}.`);
} finally {
  showcaseClient?.close();
  panelClient?.close();
  serviceWorkerClient?.close();
  if (edge?.pid !== undefined) {
    spawnSync("taskkill", ["/PID", String(edge.pid), "/T", "/F"], {
      stdio: "ignore",
      windowsHide: true,
    });
  }
  await delay(500);
  for (let attempt = 0; attempt < 12; attempt += 1) {
    try {
      await rm(temporaryRoot, { recursive: true, force: true });
      break;
    } catch (error) {
      if (error?.code !== "EBUSY" || attempt === 11) throw error;
      await delay(500);
    }
  }
}
