import { useRef, useState } from "react";
import { Gauge } from "lucide-react";
import { settingsStore, updateSettings } from "../../lib/store";
import { api } from "../../lib/api";
import * as fmt from "../../lib/format";
import { Button, Icon, Input } from "../../ui/primitives";
import { Popover } from "../../ui/overlays";
import { run } from "./actions";

export function SpeedControl() {
  const s = settingsStore.use();
  const ref = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [custom, setCustom] = useState("");
  const [invalid, setInvalid] = useState(false);
  if (!s) return null;
  const profile = s.profiles.find((p) => p.id === s.activeProfile) ?? s.profiles[0];
  const limited = !!profile && profile.download > 0;
  const label = limited ? `${fmt.speed(profile.download)} limit` : "No limit";

  const applyCustom = async () => {
    const rate = fmt.parseRate(custom);
    if (rate == null || rate <= 0) {
      setInvalid(true);
      return;
    }
    const others = s.profiles.filter((p) => p.id !== "custom");
    await run(updateSettings({ profiles: [...others, { id: "custom", name: "Custom", download: rate, upload: 0 }], activeProfile: "custom" }), "Could not set the speed limit");
    setOpen(false);
  };

  return (
    <>
      <button ref={ref} type="button" className="speed-chip num" data-limited={limited} onClick={() => setOpen(!open)} title="Speed limit">
        <Icon icon={Gauge} size={14} />
        {label}
      </button>
      {open && (
        <Popover anchor={ref.current} onClose={() => setOpen(false)} width={260}>
          <div style={{ padding: "var(--space-2)" }}>
            <div className="menu-label">Bandwidth profile</div>
            {s.profiles.map((p) => (
              <button
                key={p.id}
                type="button"
                className="menu-item"
                data-active={p.id === s.activeProfile ? "true" : undefined}
                onClick={async () => {
                  await run(api.setProfile(p.id), "Could not change the profile");
                  void settingsStore.refresh();
                  setOpen(false);
                }}
              >
                <span style={{ flex: 1 }}>{p.name}</span>
                <span className="shortcut num">{p.download ? fmt.speed(p.download) : "Unlimited"}</span>
              </button>
            ))}
            <div className="menu-sep" />
            <form
              style={{ display: "flex", gap: 6, padding: "var(--space-1)" }}
              onSubmit={(e) => {
                e.preventDefault();
                void applyCustom();
              }}
            >
              <Input
                value={custom}
                aria-invalid={invalid}
                onChange={(e) => {
                  setCustom(e.target.value);
                  setInvalid(false);
                }}
                placeholder="Custom, e.g. 2 MB"
                style={{ height: 28 }}
              />
              <Button type="submit" size="sm">
                Set
              </Button>
            </form>
            <div className="faint" style={{ fontSize: "var(--text-2xs)", padding: "0 var(--space-1) var(--space-1)" }}>
              Applies to all downloads. Edit profiles in Settings › Speed.
            </div>
          </div>
        </Popover>
      )}
    </>
  );
}
