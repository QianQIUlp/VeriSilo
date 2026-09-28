import type { Silo } from "@verisilo/contracts";

import type { ManagedIdentityPreview } from "../../desktop-api.js";
import { describeNetwork } from "../../formatters.js";
import { siloBrowserLabel } from "../../shared/presentation.js";

import "./CreatedSiloSummary.css";

export function CreatedSiloSummary({
  silo,
  preview,
  onInspect,
  onLaunch,
  onDismiss,
  busy = false,
  launching = false,
  launchBlocked = false,
  launchBlockedReason,
}: {
  silo: Silo;
  preview: ManagedIdentityPreview | undefined;
  onInspect: () => void;
  onLaunch: () => void;
  onDismiss: () => void;
  busy?: boolean;
  launching?: boolean;
  launchBlocked?: boolean;
  launchBlockedReason?: string;
}) {
  const launchHint = launching
    ? null
    : launchBlocked
      ? (launchBlockedReason ?? "已有 Silo 正在运行。先停止它，才能打开这个 Silo。")
      : busy
        ? "正在处理其他操作，完成后可打开浏览器。"
        : null;

  return (
    <section
      aria-labelledby="created-silo-summary-title"
      className="panel created-silo-summary"
    >
      <div className="panel-heading">
        <div>
          <p className="eyebrow">托管身份浏览器 · 创建完成</p>
          <h2 id="created-silo-summary-title">「{silo.name}」已创建</h2>
          <p>
            {preview === undefined
              ? "独立 Profile 已创建，但暂未取得解析后的身份配置；可查看配置。浏览器运行观察需首次打开后核对。"
              : "以下是创建时解析的身份配置与网络声明，并非浏览器运行观察；首次打开后可检查页面实际读到的值。"}
          </p>
        </div>
      </div>

      {preview === undefined ? (
        <div className="created-silo-preview-missing" role="status">
          <strong>解析配置暂不可用</strong>
          <p>加载尚未完成或读取失败。请查看配置；这里不会用预设值代替解析结果。</p>
        </div>
      ) : null}

      <dl className="visibility-facts created-silo-facts">
        {preview !== undefined ? (
          <>
            <div>
              <dt>解析语言</dt>
              <dd>{preview.language}</dd>
            </div>
            <div>
              <dt>解析时区</dt>
              <dd>{preview.timezone}</dd>
            </div>
            <div>
              <dt>配置屏幕</dt>
              <dd>{preview.screenWidth}×{preview.screenHeight}</dd>
            </div>
          </>
        ) : null}
        <div>
          <dt>浏览器引擎</dt>
          <dd>
            {silo.engine.adapter === "camoufox"
              ? "Camoufox · 独立 Firefox"
              : siloBrowserLabel(silo)}
          </dd>
        </div>
        <div>
          <dt>网络方式（配置声明）</dt>
          <dd>{describeNetwork(silo.networkProfile)}</dd>
        </div>
        {preview !== undefined ? (
          <>
            <div>
              <dt>网络位置配置</dt>
              <dd>{preview.networkBound ? "已关联身份配置" : "未标记关联"}</dd>
            </div>
            {preview.countryCode !== null ? (
              <div>
                <dt>配置中的出口地区</dt>
                <dd>{preview.countryCode}</dd>
              </div>
            ) : null}
            {preview.publicAddress !== null ? (
              <div>
                <dt>预期出口地址</dt>
                <dd>{preview.publicAddress}</dd>
              </div>
            ) : null}
          </>
        ) : null}
        <div className="created-silo-wide-fact">
          <dt>独立 Profile</dt>
          <dd>此 Silo 独立保存 Cookie、登录状态和网站数据。</dd>
        </div>
        <div className="created-silo-wide-fact">
          <dt>身份锁定边界</dt>
          <dd>
            {silo.identityLockedAt === null
              ? "首次成功启动后锁定浏览器身份配置与运行位置；启动前可在配置中查看或换一套指纹。"
              : "浏览器身份配置与运行位置已锁定；更换身份需创建新的 Silo。"}
          </dd>
        </div>
      </dl>
      <p className="form-hint created-silo-network-note">
        {preview === undefined
          ? "网络方式来自 Silo 配置，实际出口需启动后核对。"
          : "网络方式、预期出口和关联状态均来自配置，不能代替启动后的实际出口核对。"}
      </p>
      <div className="created-silo-actions">
        <button
          className="button-secondary"
          onClick={onInspect}
          type="button"
        >
          查看配置
        </button>
        <button
          aria-describedby={launchHint === null ? undefined : "created-silo-launch-hint"}
          disabled={busy || launching || launchBlocked}
          onClick={onLaunch}
          type="button"
        >
          {launching ? "正在打开…" : "打开浏览器"}
        </button>
        <button
          className="button-secondary"
          onClick={onDismiss}
          type="button"
        >
          关闭摘要
        </button>
      </div>
      {launchHint !== null ? (
        <p className="form-hint created-silo-launch-hint" id="created-silo-launch-hint" role="status">
          {launchHint}
        </p>
      ) : null}
    </section>
  );
}
