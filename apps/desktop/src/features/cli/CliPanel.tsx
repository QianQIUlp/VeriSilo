import { type Notice } from "../../shared/notice.js";

import { useEffect, useState } from "react";

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

  useEffect(() => {
    void desktopApi
      .localApiInfo()
      .then((info) => {
        setCliInfo({ cliPath: info.cliPath, vaultName: info.vaultName });
      })
      .catch(() => {
        setCliInfo(null);
      });
  }, []);

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

  return (
    <div className="settings-stack">
      <section className="panel settings-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">命令行</p>
            <h1>用命令完成浏览器工作</h1>
            <p>
              CLI 会按需启动后台服务。不同 Vault
              可以同时运行；口令只在终端隐藏输入，不会写进命令参数。
            </p>
            {cliInfo !== null ? <p>当前界面使用：{cliInfo.vaultName}</p> : null}
          </div>
        </div>
        {cliInfo !== null ? (
          <div className="cli-path-row">
            <label>
              命令文件
              <input readOnly spellCheck={false} value={cliInfo.cliPath} />
            </label>
            <button
              className="button-secondary"
              disabled={busy}
              onClick={() =>
                copyToClipboard(quoted, "已复制命令文件位置。", onNotice)
              }
              type="button"
            >
              复制位置
            </button>
          </div>
        ) : (
          <div className="empty-inline">
            <strong>还不能用命令</strong>
            <span>确认 VeriSilo 正在运行，然后回到这一页。</span>
          </div>
        )}
      </section>
      {cliInfo !== null ? (
        <section className="panel settings-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">常用命令</p>
              <h2>先看懂要做什么，再复制</h2>
              <p>把 agent 换成 Vault 名称，把「名称」换成 Silo 名字。</p>
            </div>
          </div>
          <ul className="command-list">
            {commands.map((command) => (
              <li className="command-row" key={command.label}>
                <div>
                  <strong>{command.label}</strong>
                  <code>{command.preview}</code>
                </div>
                <button
                  className="button-secondary"
                  disabled={busy}
                  onClick={() =>
                    copyToClipboard(command.value, "已复制这条命令。", onNotice)
                  }
                  type="button"
                >
                  复制
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}

function copyToClipboard(
  text: string,
  ok: string,
  onNotice: (notice: Notice) => void,
) {
  void navigator.clipboard.writeText(text).then(
    () => onNotice({ tone: "success", message: ok }),
    () =>
      onNotice({
        tone: "error",
        message: "复制没有成功，请手动选中文字。",
      }),
  );
}

function cliFileName(path: string): string {
  const parts = path.split(/[/\\]/u);
  return parts[parts.length - 1] ?? path;
}
