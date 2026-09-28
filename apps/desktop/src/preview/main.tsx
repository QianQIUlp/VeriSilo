import { useState } from "react";
import { createRoot } from "react-dom/client";
import { App } from "../App.js";
import { ManagedSiloForm } from "../features/identity/ManagedSiloForm.js";
import { installPreviewApi } from "./api.js";
import "../styles.css";
import "../shared/spatial.css";
import "../shared/living.css";

const scenario =
  new URLSearchParams(window.location.search).get("scenario") ?? "overview";
installPreviewApi(scenario);

function Preview() {
  const [message, setMessage] = useState("");
  return (
    <>
      <aside className="preview-toolbar" aria-label="UI 预览">
        <strong>UI 预览 · 模拟数据 · 不启动浏览器或读取 Vault</strong>
        <nav aria-label="预览场景">
          {Object.entries({
            overview: "概览",
            empty: "空列表",
            locked: "锁定",
            loading: "加载中",
            uninitialized: "首次使用",
            running: "运行中",
            concurrent: "双 Managed 并发",
            "concurrent-slow": "并发迟到响应",
            "concurrent-fault": "并发单侧故障",
            error: "启动失败",
            matched: "身份匹配",
            mismatched: "不匹配",
            unavailable: "无法观测",
            stale: "绑定失效",
            "recheck-failed": "复查无新证据",
            "complex-mismatch": "复杂字段差异",
            "network-expired": "网络证据到期",
            "binding-mismatch": "跨运行证据",
            "recent-run": "每 Silo 最近运行",
            "history-error": "记录读取失败",
            "history-save-failed": "记录保存失败",
            "long-name": "长名称",
            managed: "托管创建表单",
          }).map(([value, label]) => (
            <a
              key={value}
              href={`?scenario=${value}`}
              style={{ marginRight: 16 }}
            >
              {label}
            </a>
          ))}
        </nav>
      </aside>
      {scenario === "managed" ? (
        <main className="preview-standalone">
          {message && <p role="status">{message}</p>}
          <ManagedSiloForm
            busy={false}
            initialColor="#1553ff"
            managedEngineReady
            onSubmit={async () => {
              setMessage("模拟创建完成。表单未写入 Vault。");
            }}
          />
        </main>
      ) : (
        <App />
      )}
    </>
  );
}

createRoot(document.getElementById("root")!).render(<Preview />);
