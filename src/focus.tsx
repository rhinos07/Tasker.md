// Fokus-Timer (SPEC 6.5): Pomodoro oder Stoppuhr an einer Aufgabe, Pause und Stopp.
// Der Timer läuft in der Oberfläche und übersteht ein Neuladen (localStorage).
// Abgeschlossene Arbeitsphasen werden über `focus_finish` verbucht (lokal und ins Vault-Log).

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import type { TaskRef } from "./types";

export interface FocusSettings {
  enabled: boolean;
  pomodoro: { work: number; break: number };
  writeToVault: boolean;
  logSection: string;
  minMinutes: number;
  roundToMinutes: number;
}

export interface LogChange {
  path: string;
  old: string | null;
  new: string;
  addedHeading: boolean;
}

export interface FinishResult {
  minutes: number;
  log: LogChange | null;
  logNote: string | null;
}

type Mode = "pomodoro" | "stopwatch";
type Phase = "work" | "break";

interface TimerState {
  task: TaskRef;
  mode: Mode;
  phase: Phase;
  /** Startzeit der laufenden Strecke (ms), `null` = pausiert */
  runningSince: number | null;
  /** bereits gelaufene Zeit der Phase vor der laufenden Strecke */
  accumulatedMs: number;
  /** Beginn der Arbeitsphase, `HH:MM` */
  sessionStart: string;
  workMin: number;
  breakMin: number;
}

const STORAGE = "tasker.focus";

const hmNow = () => {
  const d = new Date();
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
};
const elapsed = (s: TimerState, now: number) => s.accumulatedMs + (s.runningSince ? now - s.runningSince : 0);
export const clock = (ms: number) => {
  const t = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(t / 60);
  return `${String(m).padStart(2, "0")}:${String(t % 60).padStart(2, "0")}`;
};

async function notify(title: string, body: string) {
  try {
    let ok = await isPermissionGranted();
    if (!ok) ok = (await requestPermission()) === "granted";
    if (ok) sendNotification({ title, body });
  } catch {
    // Benachrichtigungen sind optional
  }
}

export interface FocusApi {
  settings: FocusSettings | null;
  state: TimerState | null;
  now: number;
  start: (t: TaskRef) => void;
  pause: () => void;
  resume: () => void;
  stop: () => void;
  skipBreak: () => void;
  toStopwatch: () => void;
}

/**
 * Timer-Logik. `onFinished` meldet eine verbuchte Arbeitsphase (für Toast mit Rückgängig und „Erledigt abhaken?“).
 */
export function useFocus(
  demo: boolean,
  onFinished: (task: TaskRef, r: FinishResult, natural: boolean) => void,
  onError: (msg: string) => void,
): FocusApi {
  const [settings, setSettings] = useState<FocusSettings | null>(null);
  const [state, setState] = useState<TimerState | null>(() => {
    try {
      return JSON.parse(localStorage.getItem(STORAGE) ?? "null") as TimerState | null;
    } catch {
      return null;
    }
  });
  const [now, setNow] = useState(Date.now());
  const finishing = useRef(false);

  useEffect(() => {
    void invoke<FocusSettings>("focus_settings", { demo }).then(setSettings, () => setSettings(null));
  }, [demo]);

  useEffect(() => {
    if (state) localStorage.setItem(STORAGE, JSON.stringify(state));
    else localStorage.removeItem(STORAGE);
  }, [state]);

  useEffect(() => {
    if (!state?.runningSince) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [state?.runningSince]);

  const finish = useCallback(
    async (s: TimerState, ms: number, natural: boolean) => {
      const minutes = Math.floor(ms / 60000);
      const t = s.task;
      try {
        const r = await invoke<FinishResult>("focus_finish", {
          target: { path: t.notePath, description: t.task.description, raw: t.task.raw, taskId: t.task.id },
          start: s.sessionStart,
          minutes,
          demo,
        });
        onFinished(t, r, natural);
      } catch (e) {
        onError(String(e));
      }
    },
    [demo, onFinished, onError],
  );

  // Phasenende beim Pomodoro
  useEffect(() => {
    if (!state || state.mode !== "pomodoro" || !state.runningSince || finishing.current) return;
    const limit = (state.phase === "work" ? state.workMin : state.breakMin) * 60000;
    if (elapsed(state, now) < limit) return;
    if (state.phase === "work") {
      finishing.current = true;
      void finish(state, limit, true).finally(() => (finishing.current = false));
      void notify("Fokus beendet", `${state.task.task.description}: ${state.workMin} min. Erledigt abhaken?`);
      setState(
        state.breakMin > 0
          ? { ...state, phase: "break", runningSince: Date.now(), accumulatedMs: 0 }
          : null,
      );
    } else {
      void notify("Pause vorbei", "Bereit für die nächste Runde?");
      setState(null);
    }
  }, [now, state, finish]);

  // Restzeit im Fenstertitel
  useEffect(() => {
    let w: ReturnType<typeof getCurrentWindow>;
    try {
      w = getCurrentWindow();
    } catch {
      return; // außerhalb von Tauri (z. B. Browser-Vorschau)
    }
    if (!state) {
      void w.setTitle("Tasker.md").catch(() => undefined);
      return;
    }
    const ms = elapsed(state, now);
    const shown = state.mode === "pomodoro" ? (state.phase === "work" ? state.workMin : state.breakMin) * 60000 - ms : ms;
    const label = state.phase === "break" ? "Pause" : state.task.task.description;
    void w.setTitle(`${state.runningSince ? "" : "⏸ "}${clock(shown)} · ${label} – Tasker.md`).catch(() => undefined);
  }, [state, now]);

  const start = (t: TaskRef) => {
    if (!settings) return;
    if (state?.phase === "work") {
      // laufende Sitzung zuerst verbuchen
      void finish(state, elapsed(state, Date.now()), false);
    }
    setNow(Date.now());
    setState({
      task: t,
      mode: "pomodoro",
      phase: "work",
      runningSince: Date.now(),
      accumulatedMs: 0,
      sessionStart: hmNow(),
      workMin: settings.pomodoro.work,
      breakMin: settings.pomodoro.break,
    });
  };

  return {
    settings,
    state,
    now,
    start,
    pause: () => state?.runningSince && setState({ ...state, accumulatedMs: elapsed(state, Date.now()), runningSince: null }),
    resume: () => state && !state.runningSince && setState({ ...state, runningSince: Date.now() }),
    stop: () => {
      if (state?.phase === "work") void finish(state, elapsed(state, Date.now()), false);
      setState(null);
    },
    skipBreak: () => setState(null),
    toStopwatch: () => state && setState({ ...state, mode: "stopwatch" }),
  };
}

/** Leiste oben im Hauptbereich, solange ein Timer läuft. */
export function FocusBar({ f }: { f: FocusApi }) {
  const s = f.state;
  if (!s) return null;
  const ms = elapsed(s, f.now);
  const total = (s.phase === "work" ? s.workMin : s.breakMin) * 60000;
  const shown = s.mode === "pomodoro" ? total - ms : ms;
  const pct = s.mode === "pomodoro" ? Math.min(100, (ms / total) * 100) : 0;
  return (
    <div className={s.phase === "break" ? "focus-bar break" : "focus-bar"} role="timer">
      <span className="focus-time">{clock(shown)}</span>
      <span className="focus-label">
        {s.phase === "break" ? (
          "Pause"
        ) : (
          <>
            {s.mode === "stopwatch" ? "Stoppuhr · " : `Pomodoro ${s.workMin} min · `}
            <b>{s.task.task.description}</b> <span className="muted">{s.task.noteName}</span>
          </>
        )}
      </span>
      {s.runningSince ? <button onClick={f.pause}>Pause</button> : <button onClick={f.resume}>Weiter</button>}
      {s.phase === "work" && s.mode === "pomodoro" && (
        <button onClick={f.toStopwatch} title="Ohne festes Ende weiterlaufen lassen">
          Stoppuhr
        </button>
      )}
      {s.phase === "work" ? (
        <button onClick={f.stop} title="Beenden und Zeit verbuchen">
          Stopp
        </button>
      ) : (
        <button onClick={f.skipBreak}>Pause beenden</button>
      )}
      {s.mode === "pomodoro" && <div className="focus-progress" style={{ width: `${pct}%` }} />}
    </div>
  );
}
