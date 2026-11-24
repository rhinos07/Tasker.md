// Tastaturbedienung (SPEC 6.6): Auswahl einer Aufgabe, Kürzel, Datumsdialog und Kürzelübersicht.
//
// Die Auswahl folgt dem, was gerade sichtbar ist: Jede Aufgabenzeile trägt `data-task-key` und meldet
// sich im Register an. j/k springen in Dokumentreihenfolge durch die sichtbaren Zeilen.

import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { TaskRef } from "./types";

export const taskKey = (t: TaskRef) => `${t.notePath}:${t.task.line}`;

export interface Selection {
  selected: string | null;
  select: (key: string | null) => void;
  /** Register sichtbarer Aufgaben: Schlüssel → Aufgabe */
  registry: Map<string, TaskRef>;
}
export const SelectionContext = createContext<Selection | null>(null);

/** Schlüssel einer Projektzeile; Projekte sind auswählbar, aber keine Aufgaben. */
export const projectKey = (path: string) => `project:${path}`;
export const projectPath = (key: string | null) => (key?.startsWith("project:") ? key.slice(8) : null);

/** Auswählbare Zeile, die keine Aufgabe ist (z. B. ein Projekt). */
export function useNavItem(key: string) {
  const sel = useContext(SelectionContext);
  return {
    "data-task-key": key,
    selected: sel?.selected === key,
    onMouseDown: (e: React.MouseEvent) => {
      if (!(e.target as HTMLElement).closest("button, input, a")) sel?.select(key);
    },
  };
}

function visibleElements(): HTMLElement[] {
  const seen = new Set<string>();
  return [...document.querySelectorAll<HTMLElement>("main [data-task-key]")].filter((el) => {
    const k = el.dataset.taskKey!;
    if (el.offsetParent === null || seen.has(k)) return false;
    seen.add(k);
    return true;
  });
}

/** Sichtbare Aufgabenzeilen im Hauptbereich, in Dokumentreihenfolge. */
export function visibleKeys(): string[] {
  return visibleElements().map((el) => el.dataset.taskKey!);
}

/**
 * Erste Zeile des nächsten (`dir` = 1) bzw. vorigen (-1) Abschnitts, ausgehend von `from`.
 * Abschnitt ist die umgebende Karte; in „Demnächst“ also ein Tag.
 */
export function sectionKey(from: string | null, dir: 1 | -1): string | null {
  const els = visibleElements();
  if (!els.length) return null;
  const sec = (el: HTMLElement) => el.closest("section") ?? el.parentElement;
  const firsts: HTMLElement[] = [];
  for (const el of els) if (!firsts.length || sec(firsts[firsts.length - 1]) !== sec(el)) firsts.push(el);
  const cur = from ? els.find((el) => el.dataset.taskKey === from) : undefined;
  if (!cur) return (dir > 0 ? firsts[0] : firsts[firsts.length - 1]).dataset.taskKey!;
  const i = firsts.findIndex((el) => sec(el) === sec(cur));
  const next = firsts[Math.min(firsts.length - 1, Math.max(0, i + dir))];
  return next.dataset.taskKey!;
}

function scrollParent(el: HTMLElement | null): HTMLElement {
  for (let e = el?.parentElement; e && e.tagName !== "MAIN"; e = e.parentElement) {
    const o = getComputedStyle(e).overflowY;
    if ((o === "auto" || o === "scroll") && e.scrollHeight > e.clientHeight) return e;
  }
  return document.querySelector("main")!;
}

/** Halbe Seite weiter (`dir` = 1) bzw. zurück und die erste dann sichtbare Zeile wählen (wie Ctrl+d/u in Vim). */
export function halfPage(from: string | null, dir: 1 | -1): string | null {
  const els = visibleElements();
  if (!els.length) return null;
  const cur = from ? els.find((el) => el.dataset.taskKey === from) : undefined;
  const box = scrollParent(cur ?? els[0]);
  const before = box.scrollTop;
  box.scrollBy({ top: (dir * box.clientHeight) / 2 });
  const moved = box.scrollTop !== before;
  const top = box.getBoundingClientRect().top + (box.tagName === "MAIN" ? 40 : 0);
  const inBox = els.filter((el) => box.contains(el));
  // Ließ sich nicht mehr scrollen: an den Anfang bzw. das Ende
  const hit = moved ? inBox.find((el) => el.getBoundingClientRect().top >= top) : dir > 0 ? inBox[inBox.length - 1] : inBox[0];
  return (hit ?? inBox[0] ?? els[0]).dataset.taskKey!;
}

export function scrollToKey(key: string) {
  const el = document.querySelector<HTMLElement>(`main [data-task-key="${CSS.escape(key)}"]`);
  el?.scrollIntoView({ block: "nearest" });
}

/** Tastenübersicht in Gruppen; die Belegung folgt Omarchy: SUPER bleibt Hyprland, sonst Vim-Muster. */
export const SHORTCUT_GROUPS: [string, [string, string][]][] = [
  [
    "Ansichten",
    [
      ["Alt+1 … Alt+5", "Heute · Dashboard · Demnächst · Projekte · Jira"],
      ["Alt+6 … Alt+9", "eigene Filter in der Reihenfolge der Seitenleiste"],
      ["Ctrl+Tab · Ctrl+Shift+Tab", "nächste / vorige Ansicht"],
      ["L · H", "nächste / vorige Ansicht"],
      ["g h · g d · g n · g p · g j", "Heute · Dashboard · Demnächst · Projekte · Jira"],
    ],
  ],
  [
    "Bewegen",
    [
      ["j / k", "nächste / vorige Zeile"],
      ["J / K", "nächster / voriger Abschnitt (in Demnächst: Tag)"],
      ["g g · G", "erste / letzte Zeile"],
      ["Ctrl+d · Ctrl+u", "halbe Seite nach unten / oben"],
      ["Enter", "Aufgabe oder Projekt in Obsidian öffnen"],
      ["Esc", "Auswahl aufheben, Dialog schließen"],
    ],
  ],
  [
    "Aufgabe",
    [
      ["x", "abhaken"],
      ["t", "für heute planen (⏳), nochmal: entfernen"],
      ["d", "Datum wählen (⏳)"],
      ["1 / 2 / 0", "Priorität ⏫ / 🔼 / keine"],
      ["s", "in Arbeit ([/]), nochmal: wieder offen"],
      ["o", "in Obsidian öffnen"],
      ["f", "Fokus-Timer starten"],
    ],
  ],
  [
    "Allgemein",
    [
      [": · Ctrl+K", "Befehlspalette: Ansichten, Filter, Projekte, Aktionen"],
      ["/", "Suche"],
      ["q", "Schnelleingabe"],
      ["Ctrl+,", "Einstellungen (darin Ctrl+Tab bzw. H / L: Reiter wechseln)"],
      ["?", "diese Übersicht"],
    ],
  ],
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
        <div className="shortcut-groups">
          {SHORTCUT_GROUPS.map(([title, rows]) => (
            <section key={title}>
              <h3>{title}</h3>
              <table>
                <tbody>
                  {rows.map(([k, d]) => (
                    <tr key={k}>
                      <td>
                        <kbd>{k}</kbd>
                      </td>
                      <td>{d}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          ))}
          <section>
            <h3>Global</h3>
            <table>
              <tbody>
                <tr>
                  <td>
                    <kbd>{hotkey || "tasker-md --quick"}</kbd>
                  </td>
                  <td>
                    Schnellerfassung, auch wenn die App im Hintergrund ist
                    {!hotkey && " (unter Wayland an eine Compositor-Taste binden, z. B. SUPER+ALT+T)"}
                  </td>
                </tr>
              </tbody>
            </table>
          </section>
        </div>
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
