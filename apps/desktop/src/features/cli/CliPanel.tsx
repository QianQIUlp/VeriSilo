import { type Notice } from "../../shared/notice.js";

import { useEffect, useState, type CSSProperties } from "react";

import { InstrumentObject } from "../../shared/LivingWorkspace.js";

import { desktopApi } from "../../desktop-api.js";

export function CliPanel({
  busy,
  onNotice,
}: {
  busy: boolean;
  onNotice: (notice: Notice) => void;
}) {
  const [cliInfo, setCliInfo] = useState<{
    cliPath: string;
    vaultName: string;
  } | null>(null);

  const [group, setGroup] = useState(0);
  const [selected, setSelected] = useState(0);
  const [copied, setCopied] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let current = true;
    setLoading(true);
    void desktopApi
      .localApiInfo()
      .then((info) => {
        if (!current) return;
        setCliInfo({ cliPath: info.cliPath, vaultName: info.vaultName });
      })
      .catch(() => {
        if (current) setCliInfo(null);
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [retry]);

  const quoted = cliInfo === null ? "" : `"${cliInfo.cliPath}"`;
  const shortName =
    cliInfo === null ? "verisilo-cli.exe" : cliFileName(cliInfo.cliPath);
  const commands = [
    {
      label: "打开桌面窗口",
      preview: `${shortName} app open`,
      value: `${quoted} app open`,
    },
    {
      label: "查看这台机器上的 Vault",
      preview: `${shortName} vault list`,
      value: `${quoted} vault list`,
    },
    {
      label: "新建一个给 Agent 用的 Vault",
      preview: `${shortName} --vault agent vault init`,
      value: `${quoted} --vault agent vault init`,
    },
    {
      label: "在 Agent Vault 创建一个浏览器",
      preview: `${shortName} --vault agent create --name agent-1`,
      value: `${quoted} --vault agent create --name agent-1`,
    },
    {
      label: "批量创建浏览器",
      preview: `${shortName} --vault agent create-batch --prefix task --count 5`,
      value: `${quoted} --vault agent create-batch --prefix task --count 5`,
    },
    {
      label: "打开并控制一个浏览器",
      preview: `${shortName} --vault agent start 名称`,
      value: `${quoted} --vault agent start 名称`,
    },
    {
      label: "读取实际窗口数量和尺寸",
      preview: `${shortName} --vault agent page 名称 windows`,
      value: `${quoted} --vault agent page 名称 windows`,
    },
    {
      label: "读取页面和网站可见身份",
      preview: `${shortName} --vault agent page 名称 snapshot`,
      value: `${quoted} --vault agent page 名称 snapshot`,
    },
    {
      label: "打开指定网页",
      preview: `${shortName} --vault agent page 名称 goto https://example.com`,
      value: `${quoted} --vault agent page 名称 goto https://example.com`,
    },
    {
      label: "保存网页截图",
      preview: `${shortName} --vault agent page 名称 screenshot`,
      value: `${quoted} --vault agent page 名称 screenshot`,
    },
    {
      label: "永久删除 Silo",
      preview: `${shortName} --vault agent delete 名称 --yes`,
      value: `${quoted} --vault agent delete 名称 --yes`,
    },
    {
      label: "查看页面读到的身份",
      preview: `${shortName} identity`,
      value: `${quoted} identity`,
    },
  ];

  const groups = [
    { label: "打开与准备", code: "PREPARE", indices: [0, 1, 2] },
    { label: "创建与控制", code: "OPERATE", indices: [3, 4, 5, 8, 10] },
    { label: "观察与记录", code: "OBSERVE", indices: [6, 7, 9, 11] },
  ];
  const currentGroup = groups[group]!;
  const command = commands[selected]!;
  const position = currentGroup.indices.indexOf(selected);
  return (
    <section className="command-studio living-page">
      <header className="living-heading">
        <p className="eyebrow">05 / COMMAND STUDIO</p>
        <h1>把意图，变成指令。</h1>
        <p>选择要做的事，指令就在这里成形。</p>
      </header>
      <nav className="command-phases" aria-label="命令类别">
        {groups.map((item, index) => (
          <button
            key={item.code}
            type="button"
            aria-pressed={group === index}
            onClick={() => {
              setGroup(index);
              setSelected(item.indices[0]!);
            }}
          >
            <span>0{index + 1}</span>
            {item.label}
            <small>{item.code}</small>
          </button>
        ))}
      </nav>
      <div className="command-cockpit">
        <button
          className="command-emitter"
          type="button"
          aria-label="转动指令核心，选择下一条命令"
          style={
            { "--command-turn": `${position * 18 - 34}deg` } as CSSProperties
          }
          onClick={() =>
            setSelected(
              currentGroup.indices[
                (position + 1) % currentGroup.indices.length
              ]!,
            )
          }
        >
          <InstrumentObject kind="command" active={copied === command.label} />
          <span>点击转动 · 切换指令</span>
        </button>
        <div className="command-deck">
          <div className="command-deck-heading">
            <span>
              {String(position + 1).padStart(2, "0")} /{" "}
              {String(currentGroup.indices.length).padStart(2, "0")}
            </span>
            <div>
              <button
                type="button"
                className="button-secondary"
                aria-label="上一条命令"
                disabled={position === 0}
                onClick={() => setSelected(currentGroup.indices[position - 1]!)}
              >
                ←
              </button>
              <button
                type="button"
                className="button-secondary"
                aria-label="下一条命令"
                disabled={position === currentGroup.indices.length - 1}
                onClick={() => setSelected(currentGroup.indices[position + 1]!)}
              >
                →
              </button>
            </div>
          </div>
          <div className="command-output" key={command.label}>
            <h2>{command.label}</h2>
            <pre tabIndex={0} aria-label="命令预览">
              <code>{command.preview}</code>
            </pre>
            <p>
              {selected === 10
                ? "永久删除命令含 --yes。执行前请核对 Vault 与 Silo 名称。"
                : "把 agent 换成 Vault 名称，把「名称」换成 Silo 名字。"}
            </p>
            <button
              className="command-copy"
              disabled={busy || cliInfo === null}
              type="button"
              onClick={() =>
                void copyToClipboard(
                  command.value,
                  "已复制这条命令。",
                  onNotice,
                ).then((ok) => {
                  if (ok) setCopied(command.label);
                })
              }
            >
              {copied === command.label
                ? "已复制 · 可以带到终端"
                : "复制完整命令"}
              <span aria-hidden="true">
                {copied === command.label ? "✓" : "↗"}
              </span>
            </button>
            <small>这里只生成与复制文本，不会执行命令。</small>
          </div>
        </div>
      </div>
      <nav className="command-picks" aria-label="选择命令">
        {currentGroup.indices.map((index) => (
          <button
            type="button"
            key={commands[index]!.label}
            aria-pressed={selected === index}
            onClick={() => setSelected(index)}
          >
            {commands[index]!.label}
          </button>
        ))}
      </nav>
      <div className="command-connection">
        <div>
          <span className="connection-dot" aria-hidden="true" />
          <strong>
            {loading
              ? "正在读取命令文件…"
              : cliInfo
                ? `当前界面使用：${cliInfo.vaultName}`
                : "还不能用命令"}
          </strong>
          <p>
            CLI 会按需启动后台服务。不同 Vault
            可以同时运行；口令只在终端隐藏输入，不会写进命令参数。
          </p>
        </div>
        {cliInfo ? (
          <details>
            <summary>命令文件与路径</summary>
            <label>
              命令文件
              <input readOnly spellCheck={false} value={cliInfo.cliPath} />
            </label>
            <button
              type="button"
              className="button-secondary"
              disabled={busy}
              onClick={() =>
                void copyToClipboard(quoted, "已复制命令文件位置。", onNotice)
              }
            >
              复制位置
            </button>
          </details>
        ) : (
          <button
            type="button"
            className="button-secondary"
            disabled={loading}
            onClick={() => setRetry((value) => value + 1)}
          >
            重新读取
          </button>
        )}
      </div>
    </section>
  );
}

function copyToClipboard(
  text: string,
  ok: string,
  onNotice: (notice: Notice) => void,
) {
  return navigator.clipboard.writeText(text).then(
    () => {
      onNotice({ tone: "success", message: ok });
      return true;
    },
    () => {
      onNotice({ tone: "error", message: "复制没有成功，请手动选中文字。" });
      return false;
    },
  );
}

function cliFileName(path: string): string {
  const parts = path.split(/[/\\]/u);
  return parts[parts.length - 1] ?? path;
}
