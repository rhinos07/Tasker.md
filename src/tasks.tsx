// Gemeinsame Bausteine für Aufgabenlisten: Aktionen (Abhaken, ⏳ heute) und die Aufgabenzeile.

import { createContext, useContext } from "react";
import type { TaskRef } from "./types";

export const PRIO: Record<string, string> = { highest: "🔺", high: "⏫", medium: "🔼", low: "🔽", lowest: "⏬" };

export interface Actions {
  readOnly: boolean;
  today: string | null;
  busy: boolean;
  complete: (t: TaskRef) => void;
  toggleToday: (t: TaskRef) => void;
  /** setzt ⏳ auf heute und wartet auf das Ergebnis (für die Zeitleiste) */
  scheduleToday: (t: TaskRef) => Promise<boolean>;
  /** zählt jede Änderung am Vault hoch, damit Ansichten neu laden */
  version: number;
  focusEnabled: boolean;
  startFocus: (t: TaskRef) => void;
}
export const ActionsContext = createContext<Actions | null>(null);

export function TaskRow({ t, onDragStart }: { t: TaskRef; onDragStart?: (e: React.DragEvent) => void }) {
  const a = useContext(ActionsContext)!;
  const k = t.task;
  const plannedToday = k.scheduled !== null && k.scheduled === a.today;
  const recurring = k.recurrence !== null;
  const disabled = a.readOnly || a.busy;
  const hint = a.readOnly ? "Demo-Vault ist schreibgeschützt" : undefined;
  return (
    <li className={onDragStart ? "task draggable" : "task"} draggable={!!onDragStart} onDragStart={onDragStart}>
      <button
        className="check"
        onClick={() => a.complete(t)}
        disabled={disabled || recurring}
        title={recurring ? "Wiederkehrende Aufgaben vorerst in Obsidian abhaken" : (hint ?? "Abhaken")}
        aria-label={`Abhaken: ${k.description}`}
      >
        ☐
      </button>
      <span className="desc">{k.description}</span>
      {PRIO[k.priority] && <span className="meta">{PRIO[k.priority]}</span>}
      {k.due && <span className="meta due">📅 {k.due}</span>}
      {k.scheduled && <span className={plannedToday ? "meta today" : "meta"}>⏳ {k.scheduled}</span>}
      {k.recurrence && <span className="meta">🔁 {k.recurrence}</span>}
      {k.tags.map((tag) => (
        <span key={tag} className="tag">
          {tag}
        </span>
      ))}
      <button
        className={plannedToday ? "plan active" : "plan"}
        onClick={() => a.toggleToday(t)}
        disabled={disabled}
        title={hint ?? (plannedToday ? "⏳ heute entfernen" : "Für heute planen (⏳)")}
      >
        {plannedToday ? "⏳ heute ✕" : "⏳ heute"}
      </button>
      {a.focusEnabled && !k.done && (
        <button className="plan" onClick={() => a.startFocus(t)} title="Fokus-Timer starten">
          ▶
        </button>
      )}
      <span className="source" title={t.notePath}>
        {t.noteName}
      </span>
    </li>
  );
}

