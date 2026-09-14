import { useState } from "react";
import { createRoot } from "react-dom/client";
import { App } from "../App.js";
import { desktopApi } from "../desktop-api.js";
import { installPreviewApi } from "./api.js";
import "../styles.css";
import "../shared/spatial.css";
import "../shared/living.css";
import "./public.css";

const scenes = {
  matched: "身份匹配",
  mismatched: "出现差异",
  unavailable: "无法观测",
  stale: "证据过期",
  empty: "空白工作区",
};
const requested =
  new URLSearchParams(location.search).get("scene") ?? "matched";
const scene = Object.hasOwn(scenes, requested)
  ? (requested as keyof typeof scenes)
  : "matched";
installPreviewApi(scene);

function PublicDemo() {
  const [generation, setGeneration] = useState(0);
  return (
    <>
      <aside className="public-demo-bar" aria-label="官网交互演示">
        <div>
          <strong>交互演示</strong>
          <span>身份与证据均为模拟 · 请勿输入真实口令或代理凭据</span>
        </div>
        <div className="public-demo-controls">
          <select
            aria-label="演示场景"
            value={scene}
            onChange={(event) =>
              location.assign(
                `${location.pathname}?scene=${event.target.value}`,
              )
            }
          >
            {Object.entries(scenes).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
          <button
            onClick={() => {
              void desktopApi
                .unlockVault("verisilo-demo")
                .then(() => setGeneration((value) => value + 1));
            }}
            type="button"
          >
            解锁演示
          </button>
          <button onClick={() => location.reload()} type="button">
            重置
          </button>
        </div>
      </aside>
      <App key={generation} />
    </>
  );
}

createRoot(document.getElementById("demo-root")!).render(<PublicDemo />);
window.parent.postMessage({ type: "verisilo-demo-ready" }, location.origin);
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !document.querySelector("dialog[open]"))
    window.parent.postMessage({ type: "verisilo-demo-exit" }, location.origin);
});
