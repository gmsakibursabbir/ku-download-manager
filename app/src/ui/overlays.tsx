import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { ChevronRight, X, CircleCheck, CircleAlert, Info, TriangleAlert, type LucideIcon } from "lucide-react";
import { Icon, IconButton, Button } from "./primitives";

/* ───────────────────────── Menu ───────────────────────── */

export type MenuItem =
  | "sep"
  | { heading: string }
  | { label: string; icon?: LucideIcon; shortcut?: string; onSelect?: () => void; disabled?: boolean; danger?: boolean; checked?: boolean; submenu?: MenuItem[] };

type MenuState = { x: number; y: number; items: MenuItem[]; minWidth?: number } | null;
let menuState: MenuState = null;
const menuListeners = new Set<() => void>();
function setMenu(s: MenuState) {
  menuState = s;
  menuListeners.forEach((l) => l());
}

export function showMenu(x: number, y: number, items: MenuItem[], minWidth?: number) {
  setMenu({ x, y, items, minWidth });
}

/** Open a dropdown below an element. */
export function showMenuAt(el: HTMLElement, items: MenuItem[], align: "start" | "end" = "start") {
  const r = el.getBoundingClientRect();
  showMenu(align === "end" ? r.right : r.left, r.bottom + 4, items, align === "end" ? -1 : r.width);
}

export function closeMenu() {
  setMenu(null);
}

function place(x: number, y: number, w: number, h: number, alignEnd: boolean) {
  let left = alignEnd ? x - w : x;
  let top = y;
  if (left + w > window.innerWidth - 8) left = window.innerWidth - w - 8;
  if (top + h > window.innerHeight - 8) top = Math.max(8, y - h - 4);
  return { left: Math.max(8, left), top };
}

function MenuList({ items, x, y, minWidth, depth, onClose }: { items: MenuItem[]; x: number; y: number; minWidth?: number; depth: number; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: x, top: y });
  const [active, setActive] = useState(-1);
  const [sub, setSub] = useState<{ index: number; x: number; y: number } | null>(null);
  const actionable = items.map((it, i) => (typeof it === "object" && "label" in it && !it.disabled ? i : -1)).filter((i) => i >= 0);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setPos(place(x, y, el.offsetWidth, el.offsetHeight, minWidth === -1));
    if (depth === 0) el.focus();
  }, [x, y, minWidth, depth]);

  const select = (i: number) => {
    const it = items[i];
    if (typeof it !== "object" || !("label" in it) || it.disabled) return;
    if (it.submenu) {
      const r = ref.current!.children[i].getBoundingClientRect();
      setSub({ index: i, x: r.right - 2, y: r.top - 4 });
      return;
    }
    onClose();
    it.onSelect?.();
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const idx = actionable.indexOf(active);
      const next = e.key === "ArrowDown" ? actionable[(idx + 1) % actionable.length] : actionable[(idx - 1 + actionable.length) % actionable.length];
      setActive(next ?? -1);
    } else if (e.key === "Enter" || e.key === " " || e.key === "ArrowRight") {
      e.preventDefault();
      if (active >= 0) select(active);
    } else if (e.key === "Escape" || (e.key === "ArrowLeft" && depth > 0)) {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  };

  return (
    <>
      <div
        ref={ref}
        className="menu"
        role="menu"
        tabIndex={-1}
        style={{ left: pos.left, top: pos.top, minWidth: minWidth && minWidth > 0 ? minWidth : undefined }}
        onKeyDown={onKey}
        onContextMenu={(e) => e.preventDefault()}
      >
        {items.map((it, i) => {
          if (it === "sep") return <div key={i} className="menu-sep" />;
          if ("heading" in it) return <div key={i} className="menu-label">{it.heading}</div>;
          return (
            <button
              key={i}
              type="button"
              role="menuitem"
              className={`menu-item ${it.danger ? "is-danger" : ""}`}
              disabled={it.disabled}
              data-active={active === i || sub?.index === i ? "true" : undefined}
              onMouseEnter={() => {
                setActive(i);
                if (it.submenu) select(i);
                else setSub(null);
              }}
              onClick={() => select(i)}
            >
              {it.icon ? <Icon icon={it.icon} size={14} /> : it.checked != null ? <span style={{ width: 14, textAlign: "center" }}>{it.checked ? "✓" : ""}</span> : null}
              <span className="truncate">{it.label}</span>
              {it.shortcut && <span className="shortcut">{it.shortcut}</span>}
              {it.submenu && <Icon icon={ChevronRight} size={14} className="shortcut" />}
            </button>
          );
        })}
      </div>
      {sub && typeof items[sub.index] === "object" && "submenu" in (items[sub.index] as object) && (
        <MenuList items={(items[sub.index] as { submenu: MenuItem[] }).submenu} x={sub.x} y={sub.y} depth={depth + 1} onClose={onClose} />
      )}
    </>
  );
}

export function MenuHost() {
  const s = useSyncExternalStore(
    (l) => {
      menuListeners.add(l);
      return () => menuListeners.delete(l);
    },
    () => menuState,
  );
  useEffect(() => {
    if (!s) return;
    const close = (e: MouseEvent) => {
      if (!(e.target as HTMLElement).closest(".menu")) closeMenu();
    };
    const blur = () => closeMenu();
    window.addEventListener("mousedown", close, true);
    window.addEventListener("blur", blur);
    window.addEventListener("resize", blur);
    return () => {
      window.removeEventListener("mousedown", close, true);
      window.removeEventListener("blur", blur);
      window.removeEventListener("resize", blur);
    };
  }, [s]);
  if (!s) return null;
  return createPortal(<MenuList key={`${s.x},${s.y}`} items={s.items} x={s.x} y={s.y} minWidth={s.minWidth} depth={0} onClose={closeMenu} />, document.body);
}

/* ───────────────────────── Popover ───────────────────────── */

export function Popover({ anchor, onClose, children, width = 280, align = "end" }: { anchor: HTMLElement | null; onClose: () => void; children: ReactNode; width?: number; align?: "start" | "end" }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: 0, top: 0 });
  useLayoutEffect(() => {
    if (!anchor || !ref.current) return;
    const r = anchor.getBoundingClientRect();
    setPos(place(align === "end" ? r.right : r.left, r.bottom + 6, width, ref.current.offsetHeight, align === "end"));
  }, [anchor, width, align]);
  useEffect(() => {
    const down = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!ref.current?.contains(t) && !anchor?.contains(t) && !(t as HTMLElement).closest?.(".menu")) onClose();
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", down, true);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("mousedown", down, true);
      window.removeEventListener("keydown", key);
    };
  }, [anchor, onClose]);
  return createPortal(
    <div ref={ref} className="popover" role="dialog" style={{ left: pos.left, top: pos.top, width }}>
      {children}
    </div>,
    document.body,
  );
}

/* ───────────────────────── Dialog ───────────────────────── */

export function Dialog({
  title,
  onClose,
  children,
  footer,
  width,
  onSubmit,
}: {
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
  onSubmit?: () => void;
}) {
  const ref = useRef<HTMLFormElement>(null);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLElement>("[data-autofocus], input:not([type=checkbox]):not([type=radio]), textarea, select");
    (first ?? ref.current)?.focus();
    return () => prev?.focus?.();
  }, []);
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    }
    if (e.key === "Tab" && ref.current) {
      const f = [...ref.current.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), select, textarea, [tabindex='0']")];
      if (!f.length) return;
      const i = f.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey && i <= 0) {
        e.preventDefault();
        f[f.length - 1].focus();
      } else if (!e.shiftKey && i === f.length - 1) {
        e.preventDefault();
        f[0].focus();
      }
    }
  };
  return createPortal(
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <form
        ref={ref}
        className="dialog"
        role="dialog"
        aria-modal="true"
        tabIndex={-1}
        style={width ? { width: `min(${width}px, 100%)` } : undefined}
        onKeyDown={onKey}
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit?.();
        }}
      >
        <div className="dialog-header">
          <div className="dialog-title" style={{ flex: 1, minWidth: 0 }}>
            {title}
          </div>
          <IconButton icon={X} label="Close" size="sm" onClick={onClose} />
        </div>
        <div className="dialog-body">{children}</div>
        {footer && <div className="dialog-footer">{footer}</div>}
      </form>
    </div>,
    document.body,
  );
}

export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  danger,
  onConfirm,
  onClose,
  children,
}: {
  title: string;
  message: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
  children?: ReactNode;
}) {
  return (
    <Dialog
      title={title}
      onClose={onClose}
      width={420}
      onSubmit={() => {
        onConfirm();
        onClose();
      }}
      footer={
        <>
          <span className="spacer" />
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant={danger ? "danger" : "primary"} data-autofocus>
            {confirmLabel}
          </Button>
        </>
      }
    >
      <div className="muted" style={{ fontSize: "var(--text-sm)" }}>
        {message}
      </div>
      {children}
    </Dialog>
  );
}

/* ───────────────────────── Toasts ───────────────────────── */

export interface Toast {
  id: number;
  level: "info" | "success" | "warning" | "error";
  title: string;
  message?: string;
  actions?: { label: string; onClick: () => void; primary?: boolean }[];
  timeout?: number;
}

let toasts: Toast[] = [];
let nextId = 1;
const toastListeners = new Set<() => void>();
function setToasts(t: Toast[]) {
  toasts = t;
  toastListeners.forEach((l) => l());
}

export function toast(t: Omit<Toast, "id">): number {
  const id = nextId++;
  // Collapse identical consecutive toasts.
  const rest = toasts.filter((x) => !(x.title === t.title && x.message === t.message)).slice(-3);
  setToasts([...rest, { ...t, id }]);
  const timeout = t.timeout ?? (t.level === "error" ? 9000 : t.actions ? 10000 : 4500);
  if (timeout > 0) setTimeout(() => dismissToast(id), timeout);
  return id;
}

export function dismissToast(id: number) {
  setToasts(toasts.filter((t) => t.id !== id));
}

const TOAST_ICON: Record<Toast["level"], LucideIcon> = { info: Info, success: CircleCheck, warning: TriangleAlert, error: CircleAlert };

export function ToastHost() {
  const list = useSyncExternalStore(
    (l) => {
      toastListeners.add(l);
      return () => toastListeners.delete(l);
    },
    () => toasts,
  );
  return createPortal(
    <div className="toasts" aria-live="polite">
      {list.map((t) => (
        <div key={t.id} className="toast" data-level={t.level}>
          <Icon icon={TOAST_ICON[t.level]} />
          <div className="toast-body">
            <div className="toast-title">{t.title}</div>
            {t.message && <div className="toast-message">{t.message}</div>}
            {t.actions && (
              <div className="toast-actions">
                {t.actions.map((a) => (
                  <Button
                    key={a.label}
                    size="sm"
                    variant={a.primary ? "primary" : "secondary"}
                    onClick={() => {
                      dismissToast(t.id);
                      a.onClick();
                    }}
                  >
                    {a.label}
                  </Button>
                ))}
              </div>
            )}
          </div>
          <IconButton icon={X} label="Dismiss" size="sm" onClick={() => dismissToast(t.id)} />
        </div>
      ))}
    </div>,
    document.body,
  );
}
