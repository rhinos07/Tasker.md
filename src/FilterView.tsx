// Eigene Filter (SPEC 6.3): Abfrage bearbeiten, Ergebnis live sehen, speichern.

import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ActionsContext, TaskRow } from "./tasks";
import type { Scope, TaskRef } from "./types";

export interface SavedFilter {
  name: string;
  query: string;
}

interface FilterResult {
  tasks: TaskRef[];
  error: { message: string; pos: number } | null;
}

const HELP: [string, string][] = [
  ["kontext:@telefon", "Kontext"],
  ["tag:warten", "Tag"],
  ["bereich:arbeit", "Bereich (oder arbeit/privat)"],
  ["projekt:garten", "Projektname enthält"],
  ["pfad:10 Projekte", "Pfad enthält (Leerzeichen: in \"…\")"],
  ["prio:hoch", "hoch · mittel · niedrig · keine"],
  ["fällig:<7d", "heute · überfällig · <7d · keine · 2026-12-24"],
  ["geplant:heute", "wie fällig, für ⏳"],
  ["blockiert", "wartet auf andere Aufgabe"],
  ["inarbeit", "Status [/] in Arbeit"],
  ["warten", "Warte-Tag"],
  ["jira", "Notiz erwähnt ein Ticket"],
  ["jira:ABC-123", "zu diesem Ticket"],
  ["angebot", "freier Text in der Beschreibung"],
];

export default function FilterView({
  filter,
  all,
  scope,
  demo,
  readOnly,
  onSaved,
}: {
  /** gespeicherter Filter oder `null` für einen neuen */
  filter: SavedFilter | null;
  all: SavedFilter[];
  scope: Scope | null;
  demo: boolean;
  readOnly: boolean;
  /** neue Liste gespeichert; `open` = Name des danach anzuzeigenden Filters (`null` = keiner) */
  onSaved: (list: SavedFilter[], open: string | null) => void;
}) {
  const a = useContext(ActionsContext)!;
  const [name, setName] = useState(filter?.name ?? "");
  const [query, setQuery] = useState(filter?.query ?? "");
  const [res, setRes] = useState<FilterResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showHelp, setShowHelp] = useState(!filter);
  const seq = useRef(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setName(filter?.name ?? "");
    setQuery(filter?.query ?? "");
    setShowHelp(!filter);
    setError(null);
    if (!filter) input.current?.focus();
  }, [filter]);

  const run = useCallback(async () => {
    const n = ++seq.current;
    try {
      const r = await invoke<FilterResult>("filter_run", { query, scope, demo });
      if (n === seq.current) setRes(r);
    } catch (e) {
      if (n === seq.current) setError(String(e));
    }
  }, [query, scope, demo]);

  useEffect(() => {
    const id = window.setTimeout(() => void run(), 150);
    return () => window.clearTimeout(id);
  }, [run, a.version]);

  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("vault-changed", () => void runRef.current())
      .then((u) => (cancelled ? u() : (unlisten = u)))
      .catch(() => undefined);
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const dirty = !filter || name.trim() !== filter.name || query.trim() !== filter.query;
  const save = async (list: SavedFilter[], open: string | null) => {
    try {
      await invoke("filters_save", { filters: list, demo });
      setError(null);
      onSaved(list, open);
    } catch (e) {
      setError(String(e));
    }
  };
  const onSave = () => {
    const item = { name: name.trim(), query: query.trim() };
    const list = filter ? all.map((f) => (f.name === filter.name ? item : f)) : [...all, item];
    void save(list, item.name);
  };
  const onDelete = () => {
    if (filter && window.confirm(`Filter „${filter.name}“ löschen? Die Aufgaben selbst bleiben unverändert.`)) {
      void save(
        all.filter((f) => f.name !== filter.name),
        null,
      );
    }
  };

  const err = res?.error;
  return (
    <div className="filter-view">
      <section className="card filter-edit">
        <div className="filter-row">
          <input
            className="filter-name"
            placeholder="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            disabled={readOnly}
          />
          <input
            ref={input}
            className={err ? "filter-query invalid" : "filter-query"}
            placeholder="z. B. kontext:@telefon & !warten"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && dirty && !err && name.trim() && !readOnly && onSave()}
            spellCheck={false}
          />
          <button onClick={onSave} disabled={readOnly || !dirty || !!err || !name.trim()}>
            {filter ? "Speichern" : "Anlegen"}
          </button>
          {filter && (
            <button onClick={onDelete} disabled={readOnly} title="Filter löschen">
              Löschen
            </button>
          )}
          <button onClick={() => setShowHelp(!showHelp)} title="Felder und Operatoren">
            ?
          </button>
        </div>
        {err && (
          <div className="warn-text small">
            <code className="filter-caret">
              {query}
              {"\n"}
              {" ".repeat(err.pos)}^
            </code>
            {err.message}
          </div>
        )}
        {error && <div className="error">{error}</div>}
        {readOnly && <p className="muted small">Im Demo-Vault lassen sich Filter ausprobieren, aber nicht speichern.</p>}
        {showHelp && (
          <div className="filter-help small">
            <p className="muted">
              Verknüpfen mit <code>&amp;</code> (und, auch ein Leerzeichen), <code>|</code> (oder), <code>!</code> (nicht) und
              Klammern. Gilt für offene Aufgaben, der Schalter Arbeit/Privat oben gilt zusätzlich.
            </p>
            <ul>
              {HELP.map(([q, d]) => (
                <li key={q}>
                  <button className="link" onClick={() => setQuery(query.trim() ? `${query.trim()} & ${q}` : q)}>
                    <code>{q}</code>
                  </button>{" "}
                  <span className="muted">{d}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </section>
      {res && !err && (
        <section className="card">
          <h2>
            Treffer <span className="count">{res.tasks.length}</span>
          </h2>
          {res.tasks.length === 0 && <p className="empty">Keine passenden Aufgaben.</p>}
          <ul>
            {res.tasks.map((t) => (
              <TaskRow key={t.notePath + t.task.line} t={t} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
