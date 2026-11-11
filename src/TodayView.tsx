// Heute-Ansicht mit Tages-Zeitleiste (SPEC 5.1).
// Termine sind feste Blöcke, Aufgaben werden per Drag & Drop eingeplant. Zeitblöcke liegen nur
// im App-Datenordner, der Vault bleibt unverändert (außer ⏳ heute, wenn eine Aufgabe noch nicht geplant war).

import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import FocusTimes from "./FocusTimes";
import CalendarSetup, { type CalendarStatus } from "./CalendarSetup";
import { ActionsContext, TaskRow } from "./tasks";
import type { BlockView, CalendarEvent, Scope, TaskRef, TimeBlock, TodayData } from "./types";

const PX_PER_MIN = 1.1;
const SNAP = 15;
const MIN_GAP = 15;
const DRAG_TYPE = "application/x-tasker";

const toMin = (hm: string) => {
  const [h, m] = hm.split(":").map(Number);
  return h * 60 + m;
};
const toHm = (min: number) => `${String(Math.floor(min / 60)).padStart(2, "0")}:${String(min % 60).padStart(2, "0")}`;
const snap = (min: number) => Math.round(min / SNAP) * SNAP;
const nowMin = () => {
  const d = new Date();
  return d.getHours() * 60 + d.getMinutes();
};
const localDate = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

type Drag = { kind: "task"; task: TaskRef; onToday: boolean } | { kind: "block"; block: TimeBlock; grabMin: number };

interface Interval {
  from: number;
  to: number;
}

/** Freie Lücken zwischen Terminen und Blöcken innerhalb der Zeitleiste. */
function gaps(busy: Interval[], start: number, end: number): Interval[] {
  const sorted = busy.map((b) => ({ from: Math.max(b.from, start), to: Math.min(b.to, end) })).filter((b) => b.to > b.from);
  sorted.sort((a, b) => a.from - b.from);
  const out: Interval[] = [];
  let cur = start;
  for (const b of sorted) {
    if (b.from - cur >= MIN_GAP) out.push({ from: cur, to: b.from });
    cur = Math.max(cur, b.to);
  }
  if (end - cur >= MIN_GAP) out.push({ from: cur, to: end });
  return out;
}

const fmtDur = (min: number) => (min >= 60 ? `${Math.floor(min / 60)} h${min % 60 ? ` ${min % 60} min` : ""}` : `${min} min`);

function Timeline({
  data,
  dragRef,
  onDropAt,
  onResize,
  onRemove,
}: {
  data: TodayData;
  dragRef: React.MutableRefObject<Drag | null>;
  onDropAt: (startMin: number) => void;
  onResize: (b: TimeBlock, minutes: number) => void;
  onRemove: (b: TimeBlock) => void;
}) {
  const a = useContext(ActionsContext)!;
  const start = toMin(data.timeline.start);
  const end = toMin(data.timeline.end);
  const height = (end - start) * PX_PER_MIN;
  const [now, setNow] = useState(nowMin());
  const [hover, setHover] = useState<number | null>(null);
  const [resizing, setResizing] = useState<{ id: string; minutes: number } | null>(null);
  const area = useRef<HTMLDivElement>(null);
  const isToday = data.today === localDate();

  useEffect(() => {
    const id = window.setInterval(() => setNow(nowMin()), 60000);
    return () => window.clearInterval(id);
  }, []);

  const timed = data.events.filter((e): e is CalendarEvent & { start: string; end: string } => !e.allDay && !!e.start && !!e.end);
  const allDay = data.events.filter((e) => e.allDay);
  const minutesOf = (b: TimeBlock) => (resizing?.id === b.id ? resizing.minutes : b.minutes);
  const busy: Interval[] = [
    ...timed.map((e) => ({ from: toMin(e.start), to: toMin(e.end) })),
    ...data.blocks.map((b) => ({ from: toMin(b.block.start), to: toMin(b.block.start) + minutesOf(b.block) })),
  ];
  const free = gaps(busy, isToday ? Math.max(start, snap(now)) : start, end);
  const y = (min: number) => (min - start) * PX_PER_MIN;

  const minuteAt = (clientY: number) => {
    const rect = area.current!.getBoundingClientRect();
    const raw = start + (clientY - rect.top) / PX_PER_MIN;
    return Math.min(Math.max(snap(raw), start), end - SNAP);
  };
  const dropStart = (clientY: number) => {
    const d = dragRef.current;
    const m = minuteAt(clientY) - (d?.kind === "block" ? snap(d.grabMin) : 0);
    return Math.min(Math.max(m, start), end - SNAP);
  };

  const startResize = (e: React.PointerEvent, b: TimeBlock) => {
    e.preventDefault();
    e.stopPropagation();
    const y0 = e.clientY;
    const m0 = b.minutes;
    const max = end - toMin(b.start);
    const calc = (ev: PointerEvent) => Math.min(Math.max(snap(m0 + (ev.clientY - y0) / PX_PER_MIN), SNAP), max);
    const move = (ev: PointerEvent) => setResizing({ id: b.id, minutes: calc(ev) });
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const m = calc(ev);
      setResizing(null);
      if (m !== b.minutes) onResize(b, m);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const hours: number[] = [];
  for (let h = Math.ceil(start / 60) * 60; h <= end; h += 60) hours.push(h);

  return (
    <div className="timeline-wrap">
      {allDay.length > 0 && (
        <div className="allday">
          {allDay.map((e, i) => (
            <span key={i} className="event-chip">
              {e.title}
            </span>
          ))}
        </div>
      )}
      <div
        className="timeline"
        ref={area}
        style={{ height }}
        onDragOver={(e) => {
          if (!dragRef.current) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          setHover(dropStart(e.clientY));
        }}
        onDragLeave={() => setHover(null)}
        onDrop={(e) => {
          e.preventDefault();
          setHover(null);
          if (dragRef.current) onDropAt(dropStart(e.clientY));
        }}
      >
        {hours.map((h) => (
          <div key={h} className="hour" style={{ top: y(h) }}>
            <span>{toHm(h)}</span>
          </div>
        ))}
        {free.map((g) => (
          <div key={g.from} className="gap" style={{ top: y(g.from), height: (g.to - g.from) * PX_PER_MIN }}>
            {g.to - g.from >= 30 && <span>frei {fmtDur(g.to - g.from)}</span>}
          </div>
        ))}
        {timed.map((e, i) => (
          <div
            key={i}
            className="event"
            style={{ top: y(toMin(e.start)), height: Math.max((toMin(e.end) - toMin(e.start)) * PX_PER_MIN, 14) }}
            title={`${e.start}–${e.end} ${e.title}`}
          >
            <b>{e.start}</b> {e.title}
          </div>
        ))}
        {data.blocks.map(({ block: b, task: t }) => {
          const m = minutesOf(b);
          return (
            <div
              key={b.id}
              className="block"
              style={{ top: y(toMin(b.start)), height: Math.max(m * PX_PER_MIN, 18) }}
              draggable
              onDragStart={(e) => {
                const rect = e.currentTarget.getBoundingClientRect();
                dragRef.current = { kind: "block", block: b, grabMin: (e.clientY - rect.top) / PX_PER_MIN };
                e.dataTransfer.setData(DRAG_TYPE, b.id);
                e.dataTransfer.effectAllowed = "move";
              }}
              onDragEnd={() => (dragRef.current = null)}
              title={`${b.start}–${toHm(toMin(b.start) + m)} · ${t.noteName}`}
            >
              <div className="block-head">
                <button
                  className="check"
                  onClick={() => a.complete(t)}
                  disabled={a.readOnly || a.busy}
                  title={t.task.recurrence ? "Erledigen, nächste Wiederholung wird angelegt" : "Erledigen"}
                >
                  ☐
                </button>
                <span className="block-title">{t.task.description}</span>
                {a.focusEnabled && (
                  <button className="close" onClick={() => a.startFocus(t)} title="Fokus-Timer starten">
                    ▶
                  </button>
                )}
                <button className="close" onClick={() => onRemove(b)} title="Zeitblock entfernen (Aufgabe bleibt)">
                  ✕
                </button>
              </div>
              {m >= 30 && (
                <div className="block-meta">
                  {b.start}–{toHm(toMin(b.start) + m)} · {fmtDur(m)}
                </div>
              )}
              <div className="resize" onPointerDown={(e) => startResize(e, b)} title="Länge ziehen" />
            </div>
          );
        })}
        {hover !== null && (
          <div
            className="drop-preview"
            style={{
              top: y(hover),
              height:
                (dragRef.current?.kind === "block" ? dragRef.current.block.minutes : data.timeline.defaultBlockMinutes) *
                PX_PER_MIN,
            }}
          >
            {toHm(hover)}
          </div>
        )}
        {isToday && now >= start && now <= end && <div className="now" style={{ top: y(now) }} />}
      </div>
    </div>
  );
}

export default function TodayView({ scope, demo }: { scope: Scope | null; demo: boolean }) {
  const a = useContext(ActionsContext)!;
  const [data, setData] = useState<TodayData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const dragRef = useRef<Drag | null>(null);
  // Zeitleiste beim ersten Anzeigen einmal zur aktuellen Uhrzeit scrollen
  const timelineRef = useRef<HTMLElement | null>(null);
  const scrolledToNow = useRef(false);
  const [cal, setCal] = useState<CalendarStatus | null>(null);
  const [showCal, setShowCal] = useState(false);
  const [showTimes, setShowTimes] = useState(false);
  const [calBusy, setCalBusy] = useState(false);

  const loadSeq = useRef(0);
  const load = useCallback(async () => {
    const seq = ++loadSeq.current;
    try {
      const d = await invoke<TodayData>("get_today", { scope, demo });
      if (seq !== loadSeq.current) return; // veraltete Antwort (Filter inzwischen gewechselt)
      setData(d);
      setError(null);
    } catch (e) {
      if (seq === loadSeq.current) setError(String(e));
    }
  }, [scope, demo]);

  useEffect(() => {
    void load();
  }, [load, a.version]);

  // Immer die aktuelle load-Funktion: Ein Kalender-Abgleich, der beim Umschalten des Bereichs noch läuft,
  // darf danach nicht mit dem alten Bereich neu laden.
  const loadRef = useRef(load);
  loadRef.current = load;

  useEffect(() => {
    const wrap = timelineRef.current?.querySelector<HTMLElement>(".timeline-wrap");
    const now = wrap?.querySelector<HTMLElement>(".now");
    if (!wrap || !now || scrolledToNow.current) return;
    scrolledToNow.current = true;
    const offset = now.getBoundingClientRect().top - wrap.getBoundingClientRect().top;
    wrap.scrollTop = Math.max(0, offset - wrap.clientHeight / 3);
  }, [data]);

  // Kalender: Status holen, bei veraltetem Cache neu laden (nicht im Demo-Vault)
  const syncCalendar = useCallback(
    async (force: boolean) => {
      if (demo) return;
      try {
        const st = await invoke<CalendarStatus>("calendar_status");
        setCal(st);
        const stale =
          !st.fetchedAt || Date.now() - new Date(st.fetchedAt).getTime() > Math.max(st.refreshMinutes, 1) * 60000;
        if (st.active && (force || stale)) {
          setCalBusy(true);
          await invoke("calendar_refresh");
          setCal(await invoke<CalendarStatus>("calendar_status"));
          await loadRef.current();
        }
      } catch (e) {
        setError(String(e));
        setCal(await invoke<CalendarStatus>("calendar_status").catch(() => null));
      } finally {
        setCalBusy(false);
      }
    },
    [demo],
  );

  useEffect(() => {
    void syncCalendar(false);
    const id = window.setInterval(() => void syncCalendar(false), 5 * 60000);
    return () => window.clearInterval(id);
  }, [syncCalendar]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("vault-changed", () => void loadRef.current()).then((u) => (cancelled ? u() : (unlisten = u)));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const save = async (block: TimeBlock) => {
    try {
      await invoke("save_block", { block, demo });
    } catch (e) {
      setError(String(e));
    }
    await loadRef.current();
  };

  const onDropAt = async (startMin: number) => {
    const d = dragRef.current;
    dragRef.current = null;
    if (!d || !data?.today) return;
    if (d.kind === "block") {
      if (toHm(startMin) !== d.block.start) await save({ ...d.block, start: toHm(startMin) });
      return;
    }
    // Aufgabe noch nicht auf heute geplant → erst ⏳ heute setzen, sonst verschwindet der Block sofort wieder
    if (!d.onToday && !(await a.scheduleToday(d.task))) return;
    await save({
      id: "",
      date: data.today,
      start: toHm(startMin),
      minutes: data.timeline.defaultBlockMinutes,
      path: d.task.notePath,
      description: d.task.task.description,
    });
  };

  const remove = async (b: TimeBlock) => {
    try {
      await invoke("remove_block", { id: b.id, demo });
    } catch (e) {
      setError(String(e));
    }
    await loadRef.current();
  };

  const adopt = async (bv: BlockView) => {
    if (!data?.today) return;
    const onToday = bv.task.task.scheduled !== null && bv.task.task.scheduled <= data.today;
    if (!onToday && !(await a.scheduleToday(bv.task))) return;
    await save({ ...bv.block, id: "", date: data.today });
  };

  if (!data) return error ? <div className="error">{error}</div> : null;

  const dragTask = (t: TaskRef, onToday: boolean) =>
    !onToday && a.readOnly
      ? undefined // Demo-Vault: ⏳ kann nicht gesetzt werden
      : (e: React.DragEvent) => {
          dragRef.current = { kind: "task", task: t, onToday };
          e.dataTransfer.setData(DRAG_TYPE, t.notePath);
          e.dataTransfer.effectAllowed = "move";
        };
  const list = (title: string, tasks: TaskRef[], onToday: boolean, hint?: string) => (
    <section className="card">
      <h2>
        {title} <span className="count">{tasks.length}</span>
      </h2>
      {hint && <p className="muted small">{hint}</p>}
      {tasks.length === 0 ? (
        <p className="empty">Nichts offen.</p>
      ) : (
        <ul onDragEnd={() => (dragRef.current = null)}>
          {tasks.map((t) => (
            <TaskRow key={t.notePath + t.task.line} t={t} onDragStart={dragTask(t, onToday)} />
          ))}
        </ul>
      )}
    </section>
  );

  return (
    <>
      {error && <div className="error">{error}</div>}
      {a.focusEnabled && (
        <div className="today-bar">
          <button className={showTimes ? "active" : undefined} onClick={() => setShowTimes((x) => !x)}>
            ⏱ Fokus-Zeiten
          </button>
        </div>
      )}
      {showTimes && <FocusTimes demo={demo} />}
      <div className="today">
        <section className="card timeline-card" ref={timelineRef}>
          <h2>
            Zeitleiste <span className="count">{data.today}</span>
          </h2>
          {!demo && cal && (
            <div className="cal-bar small">
              {cal.active ? (
                <span className="muted">
                  {[cal.connected ? "Google" : null, ...cal.ics.map((f) => f.name)].filter(Boolean).join(" + ")} · Stand {cal.fetchedAt ? new Date(cal.fetchedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "–"}
                </span>
              ) : (
                <span className="muted">Kalender nicht verbunden, es werden nur deine Zeitblöcke angezeigt.</span>
              )}
              {cal.active && (
                <button onClick={() => void syncCalendar(true)} disabled={calBusy} title="Termine neu laden">
                  {calBusy ? "…" : "↻"}
                </button>
              )}
              <button onClick={() => setShowCal((x) => !x)}>{cal.active ? "Kalender …" : "Verbinden …"}</button>
            </div>
          )}
          {!demo && cal && showCal && (
            <CalendarSetup
              status={cal}
              onChanged={() => {
                void invoke<CalendarStatus>("calendar_status").then(setCal);
                void load();
              }}
            />
          )}
          <Timeline data={data} dragRef={dragRef} onDropAt={(m) => void onDropAt(m)} onResize={(b, minutes) => void save({ ...b, minutes })} onRemove={(b) => void remove(b)} />
        </section>
        <div className="today-lists">
          {data.inProgress.length > 0 && list("In Arbeit", data.inProgress, false)}
          {list("Heute geplant, ohne Uhrzeit", data.scheduled, true, "Auf die Zeitleiste ziehen, um einen Zeitblock anzulegen.")}
          {data.leftover.length > 0 && (
            <section className="card">
              <h2>
                Liegengeblieben <span className="count">{data.leftover.length}</span>
              </h2>
              <ul>
                {data.leftover.map((bv) => (
                  <li key={bv.block.id} className="task">
                    <span className="desc">{bv.task.task.description}</span>
                    <span className="meta">
                      {bv.block.date} {bv.block.start}
                    </span>
                    <button className="plan active" onClick={() => void adopt(bv)} disabled={a.busy} title="Gleiche Uhrzeit heute">
                      heute {bv.block.start}
                    </button>
                    <button className="plan active" onClick={() => void remove(bv.block)} title="Zeitblock verwerfen">
                      ✕
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
          {list("Fällig bis heute", data.due, false)}
          {list("Wichtig", data.important, false)}
        </div>
      </div>
    </>
  );
}
