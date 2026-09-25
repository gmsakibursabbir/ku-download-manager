import { t } from "../lib/i18n";
import { forwardRef, type ButtonHTMLAttributes, type ReactNode, type SelectHTMLAttributes, type InputHTMLAttributes } from "react";
import type { LucideIcon } from "lucide-react";
import type { Status } from "../lib/types";

/** The one icon wrapper: consistent size and stroke across the app. */
export function Icon({ icon: I, size = 16, className }: { icon: LucideIcon; size?: number; className?: string }) {
  return <I size={size} strokeWidth={1.75} className={className} aria-hidden="true" />;
}

type BtnVariant = "primary" | "secondary" | "ghost" | "danger";

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: BtnVariant; size?: "md" | "sm"; icon?: LucideIcon; busy?: boolean }
>(function Button({ variant = "secondary", size = "md", icon, busy, className = "", children, disabled, ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      className={`btn btn-${variant} ${size === "sm" ? "btn-sm" : ""} ${className}`}
      disabled={disabled || busy}
      {...rest}
    >
      {busy ? <span className="spinner" /> : icon ? <Icon icon={icon} size={size === "sm" ? 14 : 16} /> : null}
      {children}
    </button>
  );
});

export const IconButton = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { icon: LucideIcon; label: string; size?: "md" | "sm"; on?: boolean }
>(function IconButton({ icon, label, size = "md", on, className = "", ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      title={label}
      className={`icon-btn ${size === "sm" ? "icon-btn-sm" : ""} ${on ? "is-on" : ""} ${className}`}
      {...rest}
    >
      <Icon icon={icon} size={size === "sm" ? 14 : 16} />
    </button>
  );
});

export function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}

export function Checkbox({ checked, onChange, children, indeterminate, disabled }: { checked: boolean; onChange: (v: boolean) => void; children?: ReactNode; indeterminate?: boolean; disabled?: boolean }) {
  return (
    <label className="check">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        ref={(el) => {
          if (el) el.indeterminate = !!indeterminate;
        }}
        onChange={(e) => onChange(e.target.checked)}
      />
      {children}
    </label>
  );
}

export function Radio({ checked, onChange, children, name }: { checked: boolean; onChange: () => void; children?: ReactNode; name: string }) {
  return (
    <label className="check">
      <input type="radio" name={name} checked={checked} onChange={onChange} />
      {children}
    </label>
  );
}

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input({ className = "", ...rest }, ref) {
  return <input ref={ref} className={`input ${className}`} spellCheck={false} autoComplete="off" {...rest} />;
});

export function Select({ options, className = "", ...rest }: SelectHTMLAttributes<HTMLSelectElement> & { options: { value: string | number; label: string }[] }) {
  return (
    <select className={`select ${className}`} {...rest}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function Segmented<T extends string>({ value, onChange, options, label }: { value: T; onChange: (v: T) => void; options: { value: T; label: string }[]; label: string }) {
  return (
    <div className="segmented" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" aria-pressed={value === o.value} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Tabs<T extends string>({ value, onChange, tabs }: { value: T; onChange: (v: T) => void; tabs: { value: T; label: string; count?: number }[] }) {
  return (
    <div className="tabs" role="tablist">
      {tabs.map((t) => (
        <button key={t.value} role="tab" type="button" aria-selected={value === t.value} onClick={() => onChange(t.value)}>
          {t.label}
          {t.count != null && <span className="faint"> {t.count}</span>}
        </button>
      ))}
    </div>
  );
}

export function Progress({ value, state, indeterminate }: { value: number; state: Status | "queued"; indeterminate?: boolean }) {
  return (
    <div className="progress" data-state={state} data-indeterminate={indeterminate ? "true" : undefined} role="progressbar" aria-valuenow={Math.round(value)} aria-valuemin={0} aria-valuemax={100}>
      <span style={{ width: `${value}%` }} />
    </div>
  );
}

const STATUS_LABEL: Record<Status, string> = {
  queued: "Queued",
  downloading: "Downloading",
  processing: "Processing",
  paused: "Paused",
  seeding: "Seeding",
  completed: "Completed",
  error: "Failed",
};

export function statusLabel(s: Status): string {
  return t(STATUS_LABEL[s]);
}

export function StatusBadge({ status }: { status: Status }) {
  return (
    <span className="status" data-state={status}>
      {t(STATUS_LABEL[status])}
    </span>
  );
}

export function EmptyState({ title, text, action }: { title: string; text?: string; action?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-title">{title}</div>
      {text && <div className="empty-text">{text}</div>}
      {action}
    </div>
  );
}

export function PrefRow({ label, desc, children, stack }: { label: ReactNode; desc?: ReactNode; children?: ReactNode; stack?: boolean }) {
  return (
    <div className={`pref-row ${stack ? "stack" : ""}`}>
      <div className="pref-text">
        <div className="pref-label">{label}</div>
        {desc && <div className="pref-desc">{desc}</div>}
      </div>
      {children && <div className="pref-control">{children}</div>}
    </div>
  );
}

export function PrefGroup({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <>
      {title && <div className="pref-group-title">{title}</div>}
      <div className="pref-group">{children}</div>
    </>
  );
}

export function Notice({ level = "info", icon, title, children, action }: { level?: "info" | "warning" | "error"; icon: LucideIcon; title?: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className={`notice notice-${level}`} role={level === "error" ? "alert" : undefined}>
      <Icon icon={icon} />
      <div style={{ flex: 1, minWidth: 0 }}>
        {title && <div className="notice-title">{title}</div>}
        {children && <div>{children}</div>}
      </div>
      {action}
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd>{children}</kbd>;
}
