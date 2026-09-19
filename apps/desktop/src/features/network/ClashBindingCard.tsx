import { useId, type ReactNode } from "react";

import {
  clashControllerLabel,
  clashControllerPort,
  isClashPipeController,
  clashControllerUrl,
} from "../../proxy-presets.js";

import type { ClashBindingController } from "./useClashBinding.js";

/**
 * Shared connect → read-groups → pick-node flow for the local Clash/Mihomo
 * binding, used by the Standard create form, the managed create form, and the
 * Silo edit panel. Surface-specific copy is passed in; the guard, selection,
 * and error handling all come from the shared useClashBinding controller.
 */
export function ClashBindingCard({
  clash,
  disabled = false,
  title,
  description,
  readLabel = "连接并读取代理组",
  controllerLabel = "Clash 控制地址",
  /** "port" shows just the port portion (and locks pipe URLs), matching the managed form. */
  controllerDisplay = "raw",
  secretPlaceholder,
  controllerHint,
  nodeLabel = "固定节点",
  showNodeDelay = false,
  unboundNote,
  /**
   * Called after a manual controller-URL edit, for owners that keep a copy of
   * the binding elsewhere (the Standard form stores it on the network profile).
   */
  onChangeControllerUrl,
  children,
}: {
  clash: ClashBindingController;
  disabled?: boolean;
  title: string;
  description: ReactNode;
  readLabel?: string;
  controllerLabel?: string;
  controllerDisplay?: "raw" | "port";
  secretPlaceholder?: string;
  controllerHint?: ReactNode;
  nodeLabel?: string;
  showNodeDelay?: boolean;
  /** Shown while no groups have been read yet (before any binding exists). */
  unboundNote?: ReactNode;
  onChangeControllerUrl?: (value: string) => void;
  children?: ReactNode;
}) {
  const idPrefix = useId();
  const inputDisabled = disabled || clash.busy;
  const pipeController = isClashPipeController(clash.controllerUrl);
  const displayedController =
    controllerDisplay === "port"
      ? pipeController
        ? clashControllerLabel(clash.controllerUrl)
        : clashControllerPort(clash.controllerUrl) || clash.controllerUrl
      : clash.controllerUrl;
  const onControllerChange = (value: string) => {
    if (controllerDisplay === "port" && /^\d{2,5}$/u.test(value)) {
      const url = clashControllerUrl(Number(value));
      clash.setControllerUrl(url);
      onChangeControllerUrl?.(url);
      return;
    }
    clash.setControllerUrl(value);
    onChangeControllerUrl?.(value);
  };
  const selectedGroupNodes =
    clash.snapshot?.groups.find(
      (group) => group.name === clash.selectedGroup,
    )?.nodes ?? [];

  return (
    <div className="controller-card">
      <div className="controller-heading">
        <div>
          <strong>{title}</strong>
          <span>{description}</span>
        </div>
        <button
          className="button-secondary"
          disabled={inputDisabled}
          onClick={() => void clash.inspect()}
          type="button"
        >
          {clash.busy ? "正在读取…" : readLabel}
        </button>
      </div>
      <div className="form-grid controller-grid">
        <label htmlFor={`${idPrefix}-controller`}>
          {controllerLabel}
          <input
            autoComplete="off"
            disabled={inputDisabled}
            id={`${idPrefix}-controller`}
            onChange={(event) => onControllerChange(event.target.value.trim())}
            placeholder="可空；Clash Verge 不用填 9097"
            readOnly={controllerDisplay === "port" && pipeController}
            spellCheck={false}
            value={displayedController}
          />
        </label>
        <label htmlFor={`${idPrefix}-secret`}>
          Clash 密钥（没设过就空着）
          <input
            autoComplete="off"
            disabled={inputDisabled}
            id={`${idPrefix}-secret`}
            onChange={(event) => clash.setSecret(event.target.value)}
            placeholder={secretPlaceholder}
            type="password"
            value={clash.secret}
          />
        </label>
      </div>
      {controllerHint}
      {clash.status !== null ? (
        <p className="form-hint">{clash.status}</p>
      ) : null}
      {clash.error !== null ? (
        <p className="field-error" role="alert">
          {clash.error.message}
          {clash.error.detail !== null ? (
            <span className="error-detail">{clash.error.detail}</span>
          ) : null}
        </p>
      ) : null}
      {clash.snapshot !== null ? (
        <div className="form-grid controller-grid">
          <label htmlFor={`${idPrefix}-group`}>
            选择组
            <select
              disabled={disabled}
              id={`${idPrefix}-group`}
              onChange={(event) => clash.selectGroup(event.target.value)}
              value={clash.selectedGroup}
            >
              {clash.snapshot.groups.map((group) => (
                <option key={group.name} value={group.name}>
                  {group.name}
                </option>
              ))}
            </select>
          </label>
          <label htmlFor={`${idPrefix}-node`}>
            {nodeLabel}
            <select
              disabled={disabled}
              id={`${idPrefix}-node`}
              onChange={(event) => clash.selectNode(event.target.value)}
              value={clash.selectedNode}
            >
              {selectedGroupNodes.map((node) => (
                <option key={node.name} value={node.name}>
                  {node.name}
                  {showNodeDelay && node.delayMs !== null
                    ? ` · ${node.delayMs} ms`
                    : ""}
                </option>
              ))}
            </select>
          </label>
        </div>
      ) : (
        unboundNote
      )}
      {children}
    </div>
  );
}
