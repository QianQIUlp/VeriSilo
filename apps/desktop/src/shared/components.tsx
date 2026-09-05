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
        <span>让不同用途的浏览数据各自分开</span>
      </div>
    </div>
  );
}

export function TabButton({
  active,
  label,
  onClick,
}: {
  active: boolean;
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
      {label}
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
  return (
    <span className={`capability-state ${state}`}>
      <code>{state}</code> · {labels[state]}
    </span>
  );
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
