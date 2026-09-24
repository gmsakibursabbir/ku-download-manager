import { useState } from "react";
import { Plus, Pencil, Trash2, CalendarClock, CircleAlert } from "lucide-react";
import { api, errorText } from "../lib/api";
import { queuesStore, schedulesStore, settingsStore } from "../lib/store";
import type { Schedule } from "../lib/types";
import { Button, Checkbox, EmptyState, IconButton, Input, Notice, Segmented, Select, Switch } from "../ui/primitives";
import { Dialog, toast } from "../ui/overlays";
import { run } from "../app/downloads/actions";

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const AFTER: Record<string, string> = { none: "Do nothing", sleep: "Sleep", shutdown: "Shut down", quit: "Quit KuDownloader" };

export function describeSchedule(s: Schedule): string {
  const when = s.date
    ? `Once on ${new Date(`${s.date}T00:00`).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })}`
    : !s.days.length || s.days.length === 7
      ? "Every day"
      : s.days.length === 5 && [1, 2, 3, 4, 5].every((d) => s.days.includes(d))
        ? "Weekdays"
        : s.days
            .slice()
            .sort()
            .map((d) => DAYS[d - 1])
            .join(", ");
  return `${when} · ${s.start}${s.stop ? `–${s.stop}` : ""}`;
}

function ScheduleDialog({ initial, onClose }: { initial: Partial<Schedule>; onClose: () => void }) {
  const queues = queuesStore.use();
  const settings = settingsStore.use();
  const [s, setS] = useState<Partial<Schedule>>({ name: "Night downloads", enabled: true, queueId: "main", start: "02:00", stop: null, days: [], date: null, profile: null, after: "none", ...initial });
  const [repeat, setRepeat] = useState<"daily" | "days" | "once">(initial.date ? "once" : initial.days?.length ? "days" : "daily");
  const [error, setError] = useState<string | null>(null);
  const set = (p: Partial<Schedule>) => setS({ ...s, ...p });
  const submit = async () => {
    const payload: Partial<Schedule> = {
      ...s,
      days: repeat === "days" ? s.days : [],
      date: repeat === "once" ? s.date || new Date().toISOString().slice(0, 10) : null,
      stop: s.stop || null,
    };
    if (repeat === "days" && !payload.days?.length) {
      setError("Choose at least one day.");
      return;
    }
    try {
      await api.saveSchedule(payload);
      await schedulesStore.refresh();
      onClose();
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <Dialog
      title={initial.id ? "Edit schedule" : "New schedule"}
      onClose={onClose}
      width={520}
      onSubmit={() => void submit()}
      footer={
        <>
          <span className="spacer" />
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary">
            Save
          </Button>
        </>
      }
    >
      <div className="form-grid">
        <label>Name</label>
        <Input value={s.name ?? ""} onChange={(e) => set({ name: e.target.value })} />
        <label>Queue</label>
        <Select value={s.queueId} onChange={(e) => set({ queueId: e.target.value })} options={queues.map((q) => ({ value: q.id, label: q.name }))} />
        <label>Repeat</label>
        <Segmented
          label="Repeat"
          value={repeat}
          onChange={setRepeat}
          options={[
            { value: "daily", label: "Every day" },
            { value: "days", label: "On days" },
            { value: "once", label: "Once" },
          ]}
        />
        {repeat === "days" && (
          <>
            <label />
            <div style={{ display: "flex", gap: 4 }}>
              {DAYS.map((d, i) => {
                const on = s.days?.includes(i + 1);
                return (
                  <button
                    key={d}
                    type="button"
                    className={`chip ${on ? "chip-accent" : ""}`}
                    style={{ border: "none", height: 26, minWidth: 40, justifyContent: "center" }}
                    aria-pressed={on}
                    onClick={() => set({ days: on ? s.days!.filter((x) => x !== i + 1) : [...(s.days ?? []), i + 1] })}
                  >
                    {d}
                  </button>
                );
              })}
            </div>
          </>
        )}
        {repeat === "once" && (
          <>
            <label>Date</label>
            <Input type="date" value={s.date ?? new Date().toISOString().slice(0, 10)} onChange={(e) => set({ date: e.target.value })} style={{ width: 180 }} />
          </>
        )}
        <label>Start at</label>
        <Input type="time" value={s.start} onChange={(e) => set({ start: e.target.value })} style={{ width: 140 }} />
        <label>Stop at</label>
        <div className="input-group">
          <Checkbox checked={!!s.stop} onChange={(v) => set({ stop: v ? "07:00" : null })}>
            Pause the queue at
          </Checkbox>
          {s.stop && <Input type="time" value={s.stop} onChange={(e) => set({ stop: e.target.value })} style={{ width: 140 }} />}
        </div>
        <label>Speed</label>
        <Select value={s.profile ?? ""} onChange={(e) => set({ profile: e.target.value || null })} options={[{ value: "", label: "Keep current limit" }, ...(settings?.profiles ?? []).map((p) => ({ value: p.id, label: p.name }))]} />
        <label>When done</label>
        <Select value={s.after} onChange={(e) => set({ after: e.target.value })} options={Object.entries(AFTER).map(([value, label]) => ({ value, label }))} />
      </div>
      {s.after === "shutdown" || s.after === "sleep" ? (
        <div className="faint" style={{ fontSize: "var(--text-xs)" }}>
          You get a 60-second warning with the option to cancel before the computer {s.after === "sleep" ? "sleeps" : "shuts down"}.
        </div>
      ) : null}
      {error && (
        <Notice level="error" icon={CircleAlert}>
          {error}
        </Notice>
      )}
    </Dialog>
  );
}

export function ScheduledView() {
  const schedules = schedulesStore.use();
  const queues = queuesStore.use();
  const [editing, setEditing] = useState<Partial<Schedule> | null>(null);
  const toggle = (s: Schedule, enabled: boolean) => run(api.saveSchedule({ ...s, enabled }).then(() => schedulesStore.refresh()), "Could not update the schedule");
  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">Scheduled</span>
        <Button variant="primary" icon={Plus} onClick={() => setEditing({})}>
          New schedule
        </Button>
      </div>
      <div className="page">
        {schedules.length === 0 ? (
          <EmptyState
            title="No schedules"
            text="Start a queue at a set time — for example overnight — and optionally sleep or shut down when it finishes."
            action={
              <Button icon={CalendarClock} onClick={() => setEditing({})}>
                Create a schedule
              </Button>
            }
          />
        ) : (
          <div className="page-inner">
            <div className="pref-group">
              {schedules.map((s) => (
                <div key={s.id} className="pref-row">
                  <Switch label={`Enable ${s.name}`} checked={s.enabled} onChange={(v) => void toggle(s, v)} />
                  <div className="pref-text">
                    <div className="pref-label">{s.name}</div>
                    <div className="pref-desc">
                      {describeSchedule(s)} · {queues.find((q) => q.id === s.queueId)?.name ?? "Unknown queue"}
                      {s.after !== "none" ? ` · then ${AFTER[s.after].toLowerCase()}` : ""}
                      {s.lastStart ? ` · last run ${s.lastStart}` : ""}
                    </div>
                  </div>
                  <IconButton icon={Pencil} label="Edit" size="sm" onClick={() => setEditing(s)} />
                  <IconButton
                    icon={Trash2}
                    className="is-danger"
                    label="Delete"
                    size="sm"
                    onClick={() =>
                      void run(
                        api.deleteSchedule(s.id).then(() => {
                          toast({ level: "info", title: `Deleted “${s.name}”` });
                          return schedulesStore.refresh();
                        }),
                        "Could not delete the schedule",
                      )
                    }
                  />
                </div>
              ))}
            </div>
            <div className="faint" style={{ fontSize: "var(--text-xs)" }}>
              Schedules run while KuDownloader is open or in the tray. Enable “Start with Windows” in Settings to never miss one.
            </div>
          </div>
        )}
      </div>
      {editing && <ScheduleDialog initial={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}
