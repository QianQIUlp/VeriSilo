export function Brand() {
  return (
    <div className="brand">
      <img
        alt=""
        aria-hidden="true"
        className="brand-mark"
        src="/verisilo-mark.svg"
      />
      <div>
        <strong>VeriSilo</strong>
        <span>你的本地身份工作室</span>
      </div>
    </div>
  );
}

export function TabButton({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean;
  icon?: "atlas" | "create" | "vault" | "location" | "terminal";
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-pressed={active}
      className="tab"
      onClick={onClick}
      type="button"
    >
      {icon && (
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          width="20"
          height="20"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path
            d={
              {
                atlas: "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M17 13v8m-4-4h8",
                create: "M12 5v14M5 12h14",
                vault: "M5 3h14v18H5z M8 12h8 M12 8v8 M10 10l4 4m0-4-4 4",
                location: "M4 5h16v12H4z M8 21h8m-4-4v4",
                terminal: "m5 6 5 6-5 6m8 0h6",
              }[icon]
            }
          />
        </svg>
      )}
      <span>{label}</span>
    </button>
  );
}

export function LockedRoute({ onUnlock }: { onUnlock: () => void }) {
  return (
    <section className="panel locked-route">
      <h1>先解锁保险库</h1>
      <p>解锁后才能读取并管理你的 Silo 配置。</p>
      <button onClick={onUnlock} type="button">
        返回解锁
      </button>
    </section>
  );
}

export function StatusCard({
  detail,
  eyebrow,
  tone,
  value,
}: {
  detail: string;
  eyebrow: string;
  tone: "good" | "warn" | "neutral";
  value: string;
}) {
  return (
    <article className="status-card">
      <span className="status-label">{eyebrow}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
      <span className={`status-dot ${tone}`} aria-hidden="true" />
    </article>
  );
}

export function ResultItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

export function CapabilityState({
  state,
}: {
  state: "native" | "inherit" | "unavailable";
}) {
  const labels = {
    native: "本机原生",
    inherit: "跟随本机",
    unavailable: "当前不可用",
  } as const;
  return <span className={`capability-state ${state}`}>{labels[state]}</span>;
}

export function NetworkOption({
  checked,
  description,
  label,
  onChange,
}: {
  checked: boolean;
  description: string;
  label: string;
  onChange: () => void;
}) {
  return (
    <label className={checked ? "network-option selected" : "network-option"}>
      <input
        checked={checked}
        name="network"
        onChange={onChange}
        type="radio"
      />
      <span>
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
    </label>
  );
}
