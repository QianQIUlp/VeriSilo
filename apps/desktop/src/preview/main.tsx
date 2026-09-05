import { useState } from "react";
import { createRoot } from "react-dom/client";
import { App } from "../App.js";
import { ManagedSiloForm } from "../features/identity/ManagedSiloForm.js";
import { installPreviewApi } from "./api.js";
import "../styles.css";

const scenario =
  new URLSearchParams(window.location.search).get("scenario") ?? "overview";
installPreviewApi(scenario);

function Preview() {
  const [message, setMessage] = useState("");
  return (
    <>
      <aside className="notice info" aria-label="UI 预览">
        <strong>UI 预览 · 模拟数据 · 不启动浏览器或读取 Vault</strong>
        <nav aria-label="预览场景">
          {Object.entries({
            overview: "概览",
            empty: "空列表",
            locked: "锁定",
            uninitialized: "首次使用",
            running: "运行中",
            error: "启动失败",
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
        <main className="shell">
          {message && <p role="status">{message}</p>}
          <ManagedSiloForm
            busy={false}
            initialColor="#5b5ce2"
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
