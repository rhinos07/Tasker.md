// Befehlspalette (`:` oder Ctrl+K): eine Suchzeile über Ansichten, Filter, Projekte, Aktionen und
// Einstellungsreiter. Bedienung wie der Omarchy-Starter: tippen, mit Pfeiltasten oder Ctrl+j/k wählen, Enter.

import { useEffect, useMemo, useRef, useState } from "react";

export interface Command {
  id: string;
  group: string;
  label: string;
  /** Taste, die dasselbe direkt auslöst (nur zur Anzeige) */
  key?: string;
  run: () => void;
}

/**
 * Unscharfe Suche: Alle Zeichen der Eingabe müssen in dieser Reihenfolge vorkommen.
 * Treffer am Wortanfang und zusammenhängende Treffer zählen mehr. `null` = kein Treffer.
 */
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return 0;
  const t = text.toLowerCase();
  let score = 0;
  let last = -2;
  let ti = 0;
  for (const c of q) {
    const i = t.indexOf(c, ti);
    if (i < 0) return null;
    if (i === last + 1) score += 3;
    if (i === 0 || /[\s·:/(-]/.test(t[i - 1])) score += 2;
    score -= Math.min(i - ti, 5) * 0.1;
    last = i;
    ti = i + 1;
  }
  return score;
}

export default function CommandPalette({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => input.current?.focus(), []);

  const hits = useMemo(() => {
    if (!query.trim()) return commands;
    return commands
      .map((c) => ({ c, s: fuzzyScore(query, `${c.group} ${c.label}`) }))
      .filter((x): x is { c: Command; s: number } => x.s !== null)
      .sort((a, b) => b.s - a.s)
      .map((x) => x.c);
  }, [commands, query]);

  useEffect(() => setIndex(0), [query]);
  useEffect(() => {
    list.current?.querySelector<HTMLElement>(`[data-i="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const run = (c: Command | undefined) => {
    if (!c) return;
    onClose();
    c.run();
  };
  const move = (step: number) => setIndex((i) => (hits.length ? (i + step + hits.length) % hits.length : 0));

  return (
    <div className="modal-backdrop palette-backdrop" onClick={onClose}>
      <div className="modal palette" role="dialog" aria-label="Befehlspalette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={input}
          value={query}
          placeholder="Ansicht, Filter, Projekt oder Aktion …"
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            const ctrl = e.ctrlKey && !e.altKey && !e.metaKey;
            if (e.key === "Escape") onClose();
            else if (e.key === "Enter") run(hits[index]);
            else if (e.key === "ArrowDown" || (ctrl && (e.key === "j" || e.key === "n"))) move(1);
            else if (e.key === "ArrowUp" || (ctrl && (e.key === "k" || e.key === "p"))) move(-1);
            else if (e.key === "Tab") move(e.shiftKey ? -1 : 1);
            else return;
            e.preventDefault();
            e.stopPropagation();
          }}
        />
        <ul ref={list} className="palette-list">
          {hits.length === 0 && <li className="muted">Nichts gefunden.</li>}
          {hits.map((c, i) => (
            <li
              key={c.id}
              data-i={i}
              className={i === index ? "active" : undefined}
              onMouseMove={() => setIndex(i)}
              onClick={() => run(c)}
            >
              <span className="palette-group">{c.group}</span>
              <span className="palette-label">{c.label}</span>
              {c.key && <kbd>{c.key}</kbd>}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
