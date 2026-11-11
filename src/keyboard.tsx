// Tastaturbedienung (SPEC 6.6): Auswahl einer Aufgabe, Kürzel, Datumsdialog und Kürzelübersicht.
//
// Die Auswahl folgt dem, was gerade sichtbar ist: Jede Aufgabenzeile trägt `data-task-key` und meldet
// sich im Register an. j/k springen in Dokumentreihenfolge durch die sichtbaren Zeilen.

import { createContext, useEffect, useRef, useState } from "react";
import type { TaskRef } from "./types";

export const taskKey = (t: TaskRef) => `${t.notePath}:${t.task.line}`;

export interface Selection {
  selected: string | null;
  select: (key: string | null) => void;
  /** Register sichtbarer Aufgaben: Schlüssel → Aufgabe */
  registry: Map<string, TaskRef>;
}
export const SelectionContext = createContext<Selection | null>(null);

/** Sichtbare Aufgabenzeilen im Hauptbereich, in Dokumentreihenfolge. */
export function visibleKeys(): string[] {
  return [...document.querySelectorAll<HTMLElement>("main [data-task-key]")]
    .filter((el) => el.offsetParent !== null)
    .map((el) => el.dataset.taskKey!)
    .filter((k, i, all) => all.indexOf(k) === i);
}

export function scrollToKey(key: string) {
  const el = document.querySelector<HTMLElement>(`main [data-task-key="${CSS.escape(key)}"]`);
  el?.scrollIntoView({ block: "nearest" });
}

export const SHORTCUTS: [string, string][] = [
  ["j / k", "Auswahl nach unten / oben"],
  ["x", "abhaken"],
  ["t", "für heute planen (⏳), nochmal: entfernen"],
  ["d", "Datum wählen (⏳)"],
  ["1 / 2 / 0", "Priorität ⏫ / 🔼 / keine"],
  ["s", "in Arbeit ([/]), nochmal: wieder offen"],
  ["o", "in Obsidian öffnen"],
  ["f", "Fokus-Timer starten"],
  ["/", "Suche"],
  ["q", "Schnelleingabe"],
  ["g h · g d · g p · g j · g n", "Heute · Dashboard · Projekte · Jira · Demnächst"],
  ["Esc", "Auswahl aufheben, Dialog schließen"],
  ["?", "diese Übersicht"],
];

export function ShortcutHelp({ hotkey, onClose }: { hotkey: string | null; onClose: () => void }) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal shortcuts" role="dialog" aria-label="Tastenkürzel" onClick={(e) => e.stopPropagation()}>
        <header>
          <h2>Tastenkürzel</h2>
          <button className="close" onClick={onClose} aria-label="Schließen">
            ✕
          </button>
        </header>
        <table>
          <tbody>
            {SHORTCUTS.map(([k, d]) => (
              <tr key={k}>
                <td>
                  <kbd>{k}</kbd>
                </td>
                <td>{d}</td>
              </tr>
            ))}
            <tr>
              <td>
                <kbd>{hotkey || "aus"}</kbd>
              </td>
              <td>global: Schnellerfassung, auch wenn die App im Hintergrund ist</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  );
}

const iso = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** Datumsdialog für ⏳: Schnellwahl oder freies Datum. 📅 ändert er nie. */
export function DateDialog({
  task,
  onPick,
  onClose,
}: {
  task: TaskRef;
  onPick: (date: string | null) => void;
  onClose: () => void;
}) {
  const [value, setValue] = useState(task.task.scheduled ?? iso(new Date()));
  const ref = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLDivElement>(null);
  // Fokus auf den Dialog, damit die Buchstaben der Schnellwahl sofort greifen
  useEffect(() => box.current?.focus(), []);
  const plus = (n: number) => {
    const d = new Date();
    d.setDate(d.getDate() + n);
    return iso(d);
  };
  const nextMonday = () => {
    const d = new Date();
    d.setDate(d.getDate() + (((8 - d.getDay()) % 7) || 7));
    return iso(d);
  };
  const quick: [string, string, string][] = [
    ["h", "Heute", plus(0)],
    ["m", "Morgen", plus(1)],
    ["ü", "Übermorgen", plus(2)],
    ["w", "Nächster Montag", nextMonday()],
  ];
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        ref={box}
        tabIndex={-1}
        className="modal date-dialog"
        role="dialog"
        aria-label="Datum wählen"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          else if (e.key === "Enter") onPick(value || null);
          else if (e.altKey || e.target === ref.current) return;
          const q = quick.find(([k]) => k === e.key);
          if (q) onPick(q[2]);
          else if (e.key === "Delete" || e.key === "Backspace") task.task.scheduled && onPick(null);
        }}
      >
        <header>
          <h2>⏳ planen</h2>
          <button className="close" onClick={onClose} aria-label="Schließen">
            ✕
          </button>
        </header>
        <p className="small">{task.task.description}</p>
        <div className="date-quick">
          {quick.map(([k, label, d]) => (
            <button key={k} onClick={() => onPick(d)}>
              {label} <kbd>{k}</kbd>
            </button>
          ))}
          {task.task.scheduled && (
            <button onClick={() => onPick(null)}>
              ⏳ entfernen <kbd>Entf</kbd>
            </button>
          )}
        </div>
        <div className="date-row">
          <input ref={ref} type="date" value={value} onChange={(e) => setValue(e.target.value)} />
          <button className="primary" onClick={() => onPick(value || null)} disabled={!value}>
            Übernehmen
          </button>
        </div>
        {task.task.due && <p className="muted small">📅 {task.task.due} bleibt unverändert.</p>}
      </div>
    </div>
  );
}
