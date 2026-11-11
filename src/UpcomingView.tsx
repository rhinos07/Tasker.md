// Ansicht „Demnächst“ (SPEC 5.5): heute plus 13 Tage als Spalten, Termine eingeblendet.
// Ziehen auf einen Tag setzt ⏳ neu, Ziehen auf „Ohne Datum“ entfernt ⏳. 📅 wird hier nie geändert.

import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ActionsContext, PRIO, useSelectable } from "./tasks";
import type { Scope, TaskRef, UpcomingData } from "./types";

const WD = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"];
const NONE = "none";

function dayLabel(iso: string, today: string): { name: string; date: string; weekend: boolean } {
  const d = new Date(`${iso}T12:00:00`);
  const diff = Math.round((d.getTime() - new Date(`${today}T12:00:00`).getTime()) / 86400000);
  const name = diff === 0 ? "Heute" : diff === 1 ? "Morgen" : WD[d.getDay()];
  return { name, date: `${d.getDate()}.${d.getMonth() + 1}.`, weekend: d.getDay() === 0 || d.getDay() === 6 };
}

function Card({
  t,
  overdue,
  draggable,
  onDragStart,
  onDragEnd,
}: {
  t: TaskRef;
  overdue?: boolean;
  draggable: boolean;
  onDragStart: (e: React.DragEvent) => void;
  onDragEnd: () => void;
}) {
  const a = useContext(ActionsContext)!;
  const { selected, ...selectable } = useSelectable(t);
  const k = t.task;
  const onlyDue = !k.scheduled && !!k.due;
  return (
    <li
      className={`up-card${draggable ? " draggable" : ""}${t.blocked ? " blocked" : ""}${selected ? " selected" : ""}`}
      {...selectable}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      title={t.notePath}
    >
      <div className="up-desc">
        <button
          className="check"
          onClick={() => a.complete(t)}
          disabled={a.readOnly || a.busy}
          aria-label={`Abhaken: ${k.description}`}
        >
          ☐
        </button>
        <span>{k.description}</span>
      </div>
      <div className="up-meta">
        {PRIO[k.priority] && <span>{PRIO[k.priority]}</span>}
        {k.due && <span className={overdue || onlyDue ? "due" : undefined}>📅 {k.due.slice(5)}</span>}
        {k.scheduled && overdue && <span className="due">⏳ {k.scheduled.slice(5)}</span>}
        {t.blocked && <span title="Wartet auf eine andere Aufgabe">⛔</span>}
        <span className="source">{t.noteName}</span>
      </div>
    </li>
  );
}

export default function UpcomingView({ scope, demo }: { scope: Scope | null; demo: boolean }) {
  const a = useContext(ActionsContext)!;
  const [data, setData] = useState<UpcomingData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const drag = useRef<{ t: TaskRef; from: string } | null>(null);
  const seq = useRef(0);

  // Nur die Antwort der jüngsten Anfrage zählt (Filter kann inzwischen gewechselt haben).
  const load = useCallback(async () => {
    const n = ++seq.current;
    try {
      const d = await invoke<UpcomingData>("get_upcoming", { scope, demo });
      if (n !== seq.current) return;
      setData(d);
      setError(null);
    } catch (e) {
      if (n === seq.current) setError(String(e));
    }
  }, [scope, demo]);

  useEffect(() => {
    void load();
  }, [load, a.version]);

  const loadRef = useRef(load);
  loadRef.current = load;
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("vault-changed", () => void loadRef.current())
      .then((u) => (cancelled ? u() : (unlisten = u)))
      .catch(() => undefined);
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (error) return <div className="error">{error}</div>;
  if (!data) return <p className="muted">Lade …</p>;

  const canDrag = !a.readOnly && !a.busy;
  const start = (t: TaskRef, from: string) => (e: React.DragEvent) => {
    drag.current = { t, from };
    e.dataTransfer.setData("text/plain", t.task.description);
    e.dataTransfer.effectAllowed = "move";
  };
  const end = () => {
    drag.current = null;
    setOver(null);
  };
  const zone = (key: string) => ({
    onDragOver: (e: React.DragEvent) => {
      if (!drag.current || drag.current.from === key) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      if (over !== key) setOver(key);
    },
    onDragLeave: (e: React.DragEvent) => {
      if (!e.currentTarget.contains(e.relatedTarget as Node)) setOver((o) => (o === key ? null : o));
    },
    onDrop: (e: React.DragEvent) => {
      e.preventDefault();
      const d = drag.current;
      end();
      if (!d || d.from === key) return;
      if (key === NONE && !d.t.task.scheduled) {
        a.notify("Diese Aufgabe hat nur ein 📅. Fälligkeiten ändert die App nicht per Drag & Drop.");
        return;
      }
      void a.scheduleOn(d.t, key === NONE ? null : key);
    },
  });

  const q = filter.trim().toLowerCase();
  const unscheduled = q
    ? data.unscheduled.filter((t) => `${t.task.description} ${t.noteName} ${t.task.tags.join(" ")}`.toLowerCase().includes(q))
    : data.unscheduled;

  return (
    <div className="upcoming">
      <section className={`card up-none${over === NONE ? " drop" : ""}`} {...zone(NONE)}>
        <h2>
          Ohne Datum <span className="count">{data.unscheduled.length}</span>
        </h2>
        <input className="up-filter" placeholder="Filtern …" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <p className="muted small">
          Auf einen Tag ziehen setzt ⏳. Hierher ziehen entfernt ⏳.
          {a.readOnly && " Demo-Vault ist schreibgeschützt."}
        </p>
        <ul className="up-list">
          {unscheduled.map((t) => (
            <Card key={t.notePath + t.task.line} t={t} draggable={canDrag} onDragStart={start(t, NONE)} onDragEnd={end} />
          ))}
        </ul>
      </section>
      <div className="up-days">
        {data.days.map((d) => {
          const l = dayLabel(d.date, data.today);
          return (
            <section
              key={d.date}
              className={`card up-day${l.weekend ? " weekend" : ""}${d.date === data.today ? " is-today" : ""}${over === d.date ? " drop" : ""}`}
              {...zone(d.date)}
            >
              <h2>
                {l.name} <span className="count">{l.date}</span>
                {d.tasks.length > 0 && <span className="count"> · {d.tasks.length}</span>}
              </h2>
              {d.events.length > 0 && (
                <ul className="up-events">
                  {d.events.map((ev, i) => (
                    <li key={i} title={ev.title}>
                      <span className="muted">{ev.allDay ? "ganztags" : ev.start}</span> {ev.title}
                    </li>
                  ))}
                </ul>
              )}
              <ul className="up-list">
                {d.tasks.map((u) => (
                  <Card
                    key={u.notePath + u.task.line}
                    t={u}
                    overdue={u.overdue}
                    draggable={canDrag}
                    onDragStart={start(u, d.date)}
                    onDragEnd={end}
                  />
                ))}
              </ul>
            </section>
          );
        })}
      </div>
      {data.later > 0 && <p className="muted small up-later">Weitere {data.later} Aufgaben mit späterem Datum.</p>}
      {!data.calendarConnected && (
        <p className="muted small up-later">Kalender nicht verbunden, Termine fehlen (Einrichtung unter Heute).</p>
      )}
    </div>
  );
}
