// Gemeinsame Bausteine für Aufgabenlisten: Aktionen (Abhaken, ⏳ heute) und die Aufgabenzeile.

import { createContext, useContext } from "react";
import type { TaskRef } from "./types";
import { SelectionContext, taskKey } from "./keyboard";

export const PRIO: Record<string, string> = { highest: "🔺", high: "⏫", medium: "🔼", low: "🔽", lowest: "⏬" };

export interface Actions {
  readOnly: boolean;
  today: string | null;
  busy: boolean;
  complete: (t: TaskRef) => void;
  toggleToday: (t: TaskRef) => void;
  /** setzt ⏳ auf heute und wartet auf das Ergebnis (für die Zeitleiste) */
  scheduleToday: (t: TaskRef) => Promise<boolean>;
  /** setzt ⏳ auf ein Datum (`YYYY-MM-DD`) oder entfernt es (`null`); 📅 bleibt unverändert */
  scheduleOn: (t: TaskRef, date: string | null) => Promise<boolean>;
  /** kurze Meldung unten einblenden */
  notify: (text: string) => void;
  /** zählt jede Änderung am Vault hoch, damit Ansichten neu laden */
  version: number;
  focusEnabled: boolean;
  startFocus: (t: TaskRef) => void;
  /** `high` (⏫), `medium` (🔼) oder `none` */
  setPriority: (t: TaskRef, p: "high" | "medium" | "none") => void;
  openInObsidian: (t: TaskRef) => void;
  /** öffnet den Datumsdialog für ⏳ */
  pickDate: (t: TaskRef) => void;
}

/** Auswahl per Klick und Anmeldung im Register; für alle Aufgabenzeilen. */
export function useSelectable(t: TaskRef) {
  const sel = useContext(SelectionContext);
  const key = taskKey(t);
  sel?.registry.set(key, t);
  return {
    "data-task-key": key,
    selected: sel?.selected === key,
    onMouseDown: (e: React.MouseEvent) => {
      if (!(e.target as HTMLElement).closest("button, input, a")) sel?.select(key);
    },
  };
}
export const ActionsContext = createContext<Actions | null>(null);

export function TaskRow({ t, onDragStart }: { t: TaskRef; onDragStart?: (e: React.DragEvent) => void }) {
  const a = useContext(ActionsContext)!;
  const { selected, ...selectable } = useSelectable(t);
  const k = t.task;
  const plannedToday = k.scheduled !== null && k.scheduled === a.today;
  const disabled = a.readOnly || a.busy;
  const hint = a.readOnly ? "Demo-Vault ist schreibgeschützt" : undefined;
  return (
    <li
      className={`task${onDragStart ? " draggable" : ""}${selected ? " selected" : ""}`}
      draggable={!!onDragStart}
      onDragStart={onDragStart}
      {...selectable}
    >
      <button
        className="check"
        onClick={() => a.complete(t)}
        disabled={disabled}
        title={hint ?? (k.recurrence ? "Abhaken, nächste Wiederholung wird angelegt" : "Abhaken")}
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
      <button className="plan" onClick={() => a.openInObsidian(t)} title="In Obsidian öffnen (o)">
        ↗
      </button>
      <span className="source" title={t.notePath}>
        {t.noteName}
      </span>
    </li>
  );
}

