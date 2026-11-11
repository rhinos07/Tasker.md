import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { EditResult, Overview, ProjectInfo, Scope, TaskRef } from "./types";

type View = "today" | "dashboard" | "projects" | "jira" | "upcoming";
type ScopeChoice = Scope | "all";

const VIEWS: { id: View; label: string; ready: boolean }[] = [
  { id: "today", label: "Heute", ready: false },
  { id: "dashboard", label: "Dashboard", ready: true },
  { id: "projects", label: "Projekte", ready: true },
  { id: "jira", label: "Jira", ready: false },
  { id: "upcoming", label: "Demnächst", ready: false },
];

const PRIO: Record<string, string> = { highest: "🔺", high: "⏫", medium: "🔼", low: "🔽", lowest: "⏬" };

interface Actions {
  readOnly: boolean;
  today: string | null;
  busy: boolean;
  complete: (t: TaskRef) => void;
  toggleToday: (t: TaskRef) => void;
}
const ActionsContext = createContext<Actions | null>(null);

function TaskRow({ t }: { t: TaskRef }) {
  const a = useContext(ActionsContext)!;
  const k = t.task;
  const plannedToday = k.scheduled !== null && k.scheduled === a.today;
  const recurring = k.recurrence !== null;
  const disabled = a.readOnly || a.busy;
  const hint = a.readOnly ? "Demo-Vault ist schreibgeschützt" : undefined;
  return (
    <li className="task">
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
      <span className="source" title={t.notePath}>
        {t.noteName}
      </span>
    </li>
  );
}

function Section({ title, tasks }: { title: string; tasks: TaskRef[] }) {
  return (
    <section className="card">
      <h2>
        {title} <span className="count">{tasks.length}</span>
      </h2>
      {tasks.length === 0 ? <p className="empty">Nichts offen.</p> : <ul>{tasks.map((t) => <TaskRow key={t.notePath + t.task.line} t={t} />)}</ul>}
    </section>
  );
}

function GroupedSection({ title, groups }: { title: string; groups: Record<string, TaskRef[]> }) {
  const entries = Object.entries(groups);
  const total = entries.reduce((n, [, v]) => n + v.length, 0);
  return (
    <section className="card">
      <h2>
        {title} <span className="count">{total}</span>
      </h2>
      {total === 0 && <p className="empty">Nichts offen.</p>}
      {entries.map(([group, tasks]) => (
        <div key={group} className="group">
          <h3>{group}</h3>
          <ul>{tasks.map((t) => <TaskRow key={t.notePath + t.task.line} t={t} />)}</ul>
        </div>
      ))}
    </section>
  );
}

function Projects({ projects }: { projects: ProjectInfo[] }) {
  const active = projects.filter((p) => p.status === "aktiv");
  return (
    <section className="card wide">
      <h2>
        Aktive Projekte <span className="count">{active.length}</span>
      </h2>
      <table>
        <thead>
          <tr>
            <th>Projekt</th>
            <th>Bereich</th>
            <th>Offen</th>
            <th>Tage bis Termin</th>
            <th>Tage seit Review</th>
            <th>Jira</th>
          </tr>
        </thead>
        <tbody>
          {active.map((p) => (
            <tr key={p.path} className={p.reviewDue ? "warn" : undefined}>
              <td>
                {p.name}
                {p.nextActions === 0 && <span className="badge alert">ohne nächste Aktion</span>}
              </td>
              <td>{p.area ?? "–"}</td>
              <td>{p.openTasks}</td>
              <td>{p.daysToDeadline ?? "–"}</td>
              <td>
                {p.daysSinceReview ?? "–"}
                {p.reviewDue && <span className="badge">Review fällig</span>}
              </td>
              <td>
                {p.jiraAssigned.map((k) => (
                  <span key={k} className="badge jira">
                    → {k}
                  </span>
                ))}
                {p.jiraMentions
                  .filter((k) => !p.jiraAssigned.includes(k))
                  .map((k) => (
                    <span key={k} className="badge muted">
                      {k}
                    </span>
                  ))}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

export default function App() {
  const [view, setView] = useState<View>("dashboard");
  const [scope, setScope] = useState<ScopeChoice>("all");
  const [demo, setDemo] = useState(false);
  const [data, setData] = useState<Overview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<{ text: string; undo?: EditResult } | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  const load = useCallback(async () => {
    try {
      const o = await invoke<Overview>("get_overview", { scope: scope === "all" ? null : scope, demo });
      setData(o);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [scope, demo]);

  useEffect(() => {
    void load();
  }, [load]);

  const showToast = (text: string, undo?: EditResult) => {
    window.clearTimeout(toastTimer.current);
    setToast({ text, undo });
    toastTimer.current = window.setTimeout(() => setToast(null), 8000);
  };

  // Jede Änderung: schreiben, Ergebnis melden, neu laden. Konflikte werden gemeldet, nie überschrieben.
  const run = async (cmd: string, args: Record<string, unknown>, done: (r: EditResult) => string) => {
    setBusy(true);
    try {
      const r = await invoke<EditResult>(cmd, { ...args, demo });
      showToast(done(r), cmd === "undo_edit" ? undefined : r);
    } catch (e) {
      showToast(String(e));
    } finally {
      setBusy(false);
      await load();
    }
  };
  const target = (t: TaskRef) => ({ path: t.notePath, line: t.task.line, raw: t.task.raw });

  const actions: Actions = {
    readOnly: data?.source === "demo",
    today: data?.index.today ?? null,
    busy,
    complete: (t) => void run("complete_task", { target: target(t) }, () => `Erledigt: ${t.task.description}`),
    toggleToday: (t) => {
      const isToday = t.task.scheduled !== null && t.task.scheduled === data?.index.today;
      void run("schedule_task", { target: target(t), todayFlag: !isToday }, () =>
        isToday ? `⏳ entfernt: ${t.task.description}` : `Für heute geplant: ${t.task.description}`,
      );
    },
  };
  const undo = (e: EditResult) => void run("undo_edit", { edit: e }, () => "Rückgängig gemacht");

  const d = data?.dashboard;
  return (
    <ActionsContext.Provider value={actions}>
    <div className="app">
      <aside>
        <div className="brand">Tasker.md</div>
        <nav>
          {VIEWS.map((v) => (
            <button key={v.id} className={view === v.id ? "active" : undefined} onClick={() => setView(v.id)} disabled={!v.ready} title={v.ready ? undefined : "folgt"}>
              {v.label}
            </button>
          ))}
        </nav>
        <div className="footer">
          {data && (
            <>
              <div>Inbox: {data.index.inboxCount}</div>
              <div className="muted">{data.source === "demo" ? "Demo-Vault" : data.vaultName}</div>
            </>
          )}
          <label className="muted">
            <input type="checkbox" checked={demo} onChange={(e) => setDemo(e.target.checked)} /> Demo-Vault
          </label>
          <button onClick={() => void load()}>Aktualisieren</button>
        </div>
      </aside>
      <main>
        <header>
          <h1>{VIEWS.find((v) => v.id === view)?.label}</h1>
          <div className="segmented" role="group" aria-label="Bereich">
            {(["work", "private", "all"] as const).map((s) => (
              <button key={s} className={scope === s ? "active" : undefined} onClick={() => setScope(s)}>
                {s === "work" ? "Arbeit" : s === "private" ? "Privat" : "Alle"}
              </button>
            ))}
          </div>
        </header>
        {error && <div className="error">{error}</div>}
        {d && view === "dashboard" && (
          <div className="grid">
            <Section title="🔼 Wichtig" tasks={d.important} />
            <Section title="Überfällig" tasks={d.overdue} />
            <Section title="Diese Woche fällig" tasks={d.dueThisWeek} />
            <GroupedSection title="Nächste Aktionen nach Kontext" groups={d.nextActions} />
            <GroupedSection title="Warten auf" groups={d.waiting} />
            <GroupedSection title="Später dran (blockiert)" groups={d.blocked} />
          </div>
        )}
        {data && view === "projects" && (
          <Projects projects={data.index.projects.filter((p) => scope === "all" || p.scope === scope)} />
        )}
        {data && data.index.warnings.length > 0 && (
          <details className="warnings">
            <summary>{data.index.warnings.length} Hinweise</summary>
            <ul>{data.index.warnings.map((w) => <li key={w}>{w}</li>)}</ul>
          </details>
        )}
      </main>
      {toast && (
        <div className="toast" role="status">
          <span>{toast.text}</span>
          {toast.undo && (
            <button onClick={() => undo(toast.undo!)} disabled={busy}>
              Rückgängig
            </button>
          )}
          <button className="close" onClick={() => setToast(null)} aria-label="Schließen">
            ✕
          </button>
        </div>
      )}
    </div>
    </ActionsContext.Provider>
  );
}
