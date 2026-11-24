import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import JiraView from "./JiraView";
import TodayView from "./TodayView";
import UpcomingView from "./UpcomingView";
import FilterView, { type SavedFilter } from "./FilterView";
import Settings, { type Tab as SettingsTab } from "./Settings";
import QuickEntry, { blockDay, type Added } from "./QuickEntry";
import { FocusBar, useFocus, type FinishResult, type LogChange } from "./focus";
import { ActionsContext, TaskRow, type Actions } from "./tasks";
import {
  DateDialog,
  SelectionContext,
  ShortcutHelp,
  halfPage,
  projectKey,
  projectPath,
  scrollToKey,
  sectionKey,
  useNavItem,
  visibleKeys,
} from "./keyboard";
import CommandPalette, { type Command } from "./CommandPalette";
import { TABS as SETTINGS_TABS } from "./Settings";
import type { EditResult, Overview, ProjectInfo, Scope, TaskRef } from "./types";

type View = "today" | "dashboard" | "projects" | "jira" | "upcoming" | "filter-new" | `filter:${string}`;
type ScopeChoice = Scope | "all";

// Reihenfolge = Alt+1 … Alt+5 (wie SUPER+1 … für die Arbeitsbereiche in Omarchy)
const VIEWS: { id: View; label: string; ready: boolean; g: string }[] = [
  { id: "today", label: "Heute", ready: true, g: "h" },
  { id: "dashboard", label: "Dashboard", ready: true, g: "d" },
  { id: "upcoming", label: "Demnächst", ready: true, g: "n" },
  { id: "projects", label: "Projekte", ready: true, g: "p" },
  { id: "jira", label: "Jira", ready: true, g: "j" },
];
/** Eigene Filter folgen auf Alt+6 … Alt+9. */
const FILTER_SLOTS = 9 - VIEWS.length;

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

const fmtH = (m: number) => (m >= 60 ? `${Math.floor(m / 60)} h ${m % 60 ? `${m % 60} min` : ""}`.trim() : `${m} min`);

function ProjectRow({ p }: { p: ProjectInfo }) {
  const { selected, ...nav } = useNavItem(projectKey(p.path));
  const cls = [p.reviewDue ? "warn" : "", selected ? "selected" : ""].filter(Boolean).join(" ");
  return (
    <tr {...nav} className={cls || undefined}>
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
      <td className="muted">{p.focusTotal > 0 ? `${fmtH(p.focusWeek)} / ${fmtH(p.focusTotal)}` : "–"}</td>
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
            <th>Fokus Woche / gesamt</th>
            <th>Jira</th>
          </tr>
        </thead>
        <tbody>
          {active.map((p) => (
            <ProjectRow key={p.path} p={p} />
          ))}
        </tbody>
      </table>
    </section>
  );
}

export default function App() {
  const [view, setView] = useState<View>("today");
  const [scope, setScope] = useState<ScopeChoice>("all");
  const [demo, setDemo] = useState(false);
  const [data, setData] = useState<Overview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [version, setVersion] = useState(0);
  const [showSettings, setShowSettings] = useState<false | SettingsTab>(false);
  const [configVersion, setConfigVersion] = useState(0);
  const [filters, setFilters] = useState<SavedFilter[]>([]);
  useEffect(() => {
    void invoke<SavedFilter[]>("filters_list", { demo }).then(setFilters, () => setFilters([]));
  }, [demo, configVersion]);
  const activeFilter = view.startsWith("filter:") ? (filters.find((f) => `filter:${f.name}` === view) ?? null) : null;

  // Erster Start ohne Konfiguration: Einrichtung öffnen
  useEffect(() => {
    void invoke<{ exists: boolean }>("get_settings").then((d) => !d.exists && setShowSettings("vault"), () => undefined);
  }, []);
  const [toast, setToast] = useState<{
    text: string;
    undo?: EditResult;
    undoLog?: LogChange;
    undoInsert?: Added;
    complete?: TaskRef;
  } | null>(null);
  const [showQuick, setShowQuick] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const registry = useRef(new Map<string, TaskRef>()).current;
  const [dateTask, setDateTask] = useState<TaskRef | null>(null);
  const [showHelp, setShowHelp] = useState(false);
  const [showPalette, setShowPalette] = useState(false);
  const [hotkey, setHotkey] = useState<string | null>(null);
  const gPending = useRef(0);
  const toastTimer = useRef<number | undefined>(undefined);

  // Nur die Antwort der jüngsten Anfrage zählt: Ein langsamer Lauf mit altem Filter darf keinen neueren überschreiben.
  const loadSeq = useRef(0);
  const load = useCallback(async () => {
    const seq = ++loadSeq.current;
    try {
      const o = await invoke<Overview>("get_overview", { scope: scope === "all" ? null : scope, demo });
      if (seq !== loadSeq.current) return;
      setData(o);
      setError(null);
    } catch (e) {
      if (seq === loadSeq.current) setError(String(e));
    }
  }, [scope, demo, configVersion]);

  useEffect(() => {
    void load();
  }, [load]);

  // Datei-Watcher: bei Änderungen im Vault automatisch neu laden (immer mit dem aktuellen Filter).
  const [live, setLive] = useState(false);
  const loadRef = useRef(load);
  useEffect(() => {
    loadRef.current = load;
  }, [load]);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void (async () => {
      try {
        await invoke("watch_vault", { demo });
        setLive(true);
      } catch {
        setLive(false);
      }
      const u = await listen<number>("vault-changed", () => void loadRef.current());
      if (cancelled) u();
      else unlisten = u;
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [demo, configVersion]);

  const showToast = useCallback(
    (text: string, undo?: EditResult, extra?: { undoLog?: LogChange; undoInsert?: Added; complete?: TaskRef }, ms = 8000) => {
      window.clearTimeout(toastTimer.current);
      setToast({ text, undo, ...extra });
      toastTimer.current = window.setTimeout(() => setToast(null), ms);
    },
    [],
  );

  // Fokus-Timer: verbuchte Sitzung melden, mit „Log rückgängig“ und (nach einem vollen Pomodoro) „Abhaken“
  const onFocusFinished = useCallback(
    (task: TaskRef, r: FinishResult, natural: boolean) => {
      if (r.minutes === 0) {
        showToast(r.logNote ?? "Unter einer Minute, nichts erfasst.");
        return;
      }
      const parts = [`Fokus ${r.minutes} min verbucht`];
      if (r.log) parts.push("ins Log geschrieben");
      if (r.logNote) parts.push(r.logNote);
      showToast(parts.join(" · "), undefined, { undoLog: r.log ?? undefined, complete: natural ? task : undefined }, 15000);
      setVersion((v) => v + 1);
      void loadRef.current();
    },
    [showToast],
  );
  const onFocusError = useCallback((m: string) => showToast(m), [showToast]);
  const focus = useFocus(demo, onFocusFinished, onFocusError, configVersion);

  const undoLog = async (c: LogChange) => {
    try {
      await invoke("focus_undo_log", { change: c });
      showToast("Log-Eintrag rückgängig gemacht (Zeit bleibt lokal erfasst).");
    } catch (e) {
      showToast(String(e));
    }
    setVersion((v) => v + 1);
  };


  const onQuickAdded = (a: Added) => {
    const where = a.createdFile ? `neue Notiz ${a.path}` : a.targetName;
    const block = a.block ? `, Zeitblock ${blockDay(a.block.date)} ${a.block.start}` : "";
    const blockErr = a.blockError ? ` (Zeitblock nicht angelegt: ${a.blockError})` : "";
    showToast(`Angelegt in ${where}${block}${blockErr}`, undefined, { undoInsert: a.createdFile ? undefined : a });
    setVersion((v) => v + 1);
    void load();
  };
  const undoInsert = async (a: Added) => {
    try {
      await invoke("quick_undo", { result: a, blockId: a.block?.id ?? null });
      showToast("Neue Aufgabe wieder entfernt.");
    } catch (e) {
      showToast(String(e));
    }
    setVersion((v) => v + 1);
    void load();
  };

  // Nach dem Log-Eintrag kann sich die Zeilennummer verschoben haben: Aufgabe frisch nachschlagen.
  const completeFresh = async (t: TaskRef) => {
    try {
      const o = await invoke<Overview>("get_overview", { scope: null, demo });
      const fresh = o.index.tasks.find((x) => x.notePath === t.notePath && x.task.raw === t.task.raw && !x.task.done);
      if (!fresh) {
        showToast("Aufgabe nicht mehr gefunden, bitte in der Liste abhaken.");
        return;
      }
      actions.complete(fresh);
    } catch (e) {
      showToast(String(e));
    }
  };

  // Jede Änderung: schreiben, Ergebnis melden, neu laden. Konflikte werden gemeldet, nie überschrieben.
  const run = async (cmd: string, args: Record<string, unknown>, done: (r: EditResult) => string): Promise<boolean> => {
    setBusy(true);
    try {
      const r = await invoke<EditResult>(cmd, { ...args, demo });
      showToast(done(r), cmd === "undo_edit" ? undefined : r);
      return true;
    } catch (e) {
      showToast(String(e));
      return false;
    } finally {
      setBusy(false);
      setVersion((v) => v + 1);
      await loadRef.current();
    }
  };
  const target = (t: TaskRef) => ({ path: t.notePath, line: t.task.line, raw: t.task.raw });

  // In Obsidian öffnen: mit Advanced URI auf die Zeile, sonst Notiz öffnen und Text in die Zwischenablage
  const openInObsidian = async (t: TaskRef) => {
    try {
      const link = await invoke<{ url: string; exact: boolean }>("open_in_obsidian", {
        path: t.notePath,
        line: t.task.line,
        demo,
      });
      if (link.exact) return;
      const copied = await navigator.clipboard.writeText(t.task.description).then(
        () => true,
        () => false,
      );
      const first = !localStorage.getItem("tasker.advUriHint");
      localStorage.setItem("tasker.advUriHint", "1");
      showToast(
        (copied ? "Notiz geöffnet, Aufgabentext liegt in der Zwischenablage (Strg/Cmd+F, dann einfügen)." : "Notiz geöffnet.") +
          (first
            ? " Tipp: Mit dem Obsidian-Plugin „Advanced URI“ springt die App direkt auf die Zeile. Achtung: Das Plugin erlaubt Links, Notizen zu ändern und Befehle auszuführen."
            : ""),
        undefined,
        undefined,
        first ? 15000 : 6000,
      );
    } catch (e) {
      showToast(String(e));
    }
  };

  const actions: Actions = {
    readOnly: data?.source === "demo",
    today: data?.index.today ?? null,
    busy,
    version,
    focusEnabled: !!focus.settings?.enabled,
    startFocus: (t) => {
      focus.start(t);
      // optional: Fokus-Start markiert die Aufgabe als in Arbeit
      if (focus.settings?.markInProgress && data?.source !== "demo" && t.task.status === " ") {
        void run("set_status", { target: target(t), status: "inProgress" }, () => `Fokus gestartet, in Arbeit: ${t.task.description}`);
      }
    },
    scheduleToday: (t) =>
      run("schedule_task", { target: target(t), todayFlag: true }, () => `Für heute geplant: ${t.task.description}`),
    scheduleOn: (t, date) =>
      run("schedule_on", { target: target(t), date }, () =>
        date ? `⏳ ${date.split("-").reverse().slice(0, 2).join(".")}.: ${t.task.description}` : `⏳ entfernt: ${t.task.description}`,
      ),
    notify: (text) => showToast(text),
    setPriority: (t, p) =>
      void run("set_priority", { target: target(t), priority: p }, () =>
        p === "none" ? `Priorität entfernt: ${t.task.description}` : `${p === "high" ? "⏫" : "🔼"} ${t.task.description}`,
      ),
    toggleInProgress: (t) =>
      void run("set_status", { target: target(t), status: t.task.inProgress ? "open" : "inProgress" }, () =>
        t.task.inProgress ? `Nicht mehr in Arbeit: ${t.task.description}` : `In Arbeit: ${t.task.description}`,
      ),
    pickDate: (t) => setDateTask(t),
    openInObsidian: (t) => void openInObsidian(t),
    complete: (t) =>
      void run("complete_task", { target: target(t) }, (r) => {
        // 🔁: Die erste Zeile von r.new ist die nächste Instanz
        const next = r.new.includes("\n") ? /(?:⏳|📅|🛫)️?\s*(\d{4})-(\d{2})-(\d{2})/.exec(r.new.split("\n")[0]) : null;
        if (!next) return `Erledigt: ${t.task.description}`;
        const [, y, m, d] = next;
        const year = y === data?.index.today?.slice(0, 4) ? "" : y;
        return `Erledigt: ${t.task.description} · nächste am ${d}.${m}.${year}`;
      }),
    toggleToday: (t) => {
      const isToday = t.task.scheduled !== null && t.task.scheduled === data?.index.today;
      void run("schedule_task", { target: target(t), todayFlag: !isToday }, () =>
        isToday ? `⏳ entfernt: ${t.task.description}` : `Für heute geplant: ${t.task.description}`,
      );
    },
  };
  const undo = (e: EditResult) => void run("undo_edit", { edit: e }, () => "Rückgängig gemacht");

  // --- Tastatur (SPEC 6.6) ---
  // SUPER gehört Hyprland/dem System; die App übernimmt nur dessen Muster: Alt+Ziffer wie SUPER+Ziffer,
  // Ctrl+Tab wie SUPER+Tab, dazu Vim-Tasten (j/k, J/K, gg/G, Ctrl+d/u, H/L wie Puffer in LazyVim).
  const viewOrder: View[] = [...VIEWS.map((v) => v.id), ...filters.map((f) => `filter:${f.name}` as View)];
  const goView = (to: View) => {
    setView(to);
    setSelected(null);
  };
  const cycleView = (step: 1 | -1) => {
    const i = viewOrder.indexOf(view);
    goView(viewOrder[i < 0 ? (step > 0 ? 0 : viewOrder.length - 1) : (i + step + viewOrder.length) % viewOrder.length]);
  };
  const openProject = async (path: string) => {
    try {
      await invoke("open_in_obsidian", { path, line: 0, demo });
    } catch (e) {
      showToast(String(e));
    }
  };
  const selectKey = (k: string | null) => {
    if (!k) return;
    setSelected(k);
    scrollToKey(k);
  };
  const onKeyRef = useRef<(e: KeyboardEvent) => void>(() => undefined);
  onKeyRef.current = (e: KeyboardEvent) => {
    const el = e.target as HTMLElement;
    const inField = !!el.closest("input, textarea, select, [contenteditable]");
    const dialog = showSettings || showQuick || dateTask || showHelp || showPalette;
    const k = e.key;
    const ctrlOnly = e.ctrlKey && !e.altKey && !e.metaKey;
    // Palette und Einstellungen gehen auch aus einem Eingabefeld heraus
    if (ctrlOnly && !dialog && (k === "k" || k === "K" || k === ",")) {
      e.preventDefault();
      if (k === ",") setShowSettings("vault");
      else setShowPalette(true);
      return;
    }
    if (dialog) return; // Dialoge haben eigene Tasten
    if (ctrlOnly && k === "Tab") {
      e.preventDefault();
      cycleView(e.shiftKey ? -1 : 1);
      return;
    }
    // Alt+Ziffer: über e.code, weil Alt auf macOS die Zeichen verändert
    if (e.altKey && !e.ctrlKey && !e.metaKey && /^Digit[1-9]$/.test(e.code)) {
      const n = Number(e.code.slice(5));
      const to = n <= VIEWS.length ? VIEWS[n - 1].id : filters[n - VIEWS.length - 1] && (`filter:${filters[n - VIEWS.length - 1].name}` as View);
      e.preventDefault();
      if (to) goView(to);
      else showToast(`Alt+${n}: kein eigener Filter an dieser Stelle.`);
      return;
    }
    if (inField) return;
    if (ctrlOnly && (k === "d" || k === "u")) {
      e.preventDefault();
      const next = halfPage(selected, k === "d" ? 1 : -1);
      if (next) setSelected(next);
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    // g-Folgen: g h, g d, g n, g p, g j, g g
    if (gPending.current && Date.now() - gPending.current < 1200) {
      gPending.current = 0;
      e.preventDefault();
      if (k === "g") selectKey(visibleKeys()[0] ?? null);
      const to = VIEWS.find((v) => v.g === k);
      if (to) goView(to.id);
      return;
    }
    if (k === "g") {
      gPending.current = Date.now();
      return;
    }
    const nav = (step: number) => {
      const keys = visibleKeys();
      if (!keys.length) return;
      const i = selected ? keys.indexOf(selected) : -1;
      selectKey(keys[i < 0 ? (step > 0 ? 0 : keys.length - 1) : Math.min(keys.length - 1, Math.max(0, i + step))]);
    };
    const simple: Record<string, () => void> = {
      j: () => nav(1),
      ArrowDown: () => nav(1),
      k: () => nav(-1),
      ArrowUp: () => nav(-1),
      J: () => selectKey(sectionKey(selected, 1)),
      K: () => selectKey(sectionKey(selected, -1)),
      G: () => selectKey(visibleKeys().slice(-1)[0] ?? null),
      L: () => cycleView(1),
      H: () => cycleView(-1),
      q: () => setShowQuick(true),
      "/": () => goView("filter-new"),
      ":": () => setShowPalette(true),
      "?": () => setShowHelp(true),
      Escape: () => setSelected(null),
    };
    if (simple[k]) {
      e.preventDefault();
      simple[k]();
      return;
    }
    const project = projectPath(selected);
    if (k === "Enter" && !el.closest("button, a, summary")) {
      e.preventDefault();
      const t = selected && visibleKeys().includes(selected) ? registry.get(selected) : undefined;
      if (project) void openProject(project);
      else if (t) actions.openInObsidian(t);
      else showToast("Erst eine Zeile mit j/k oder einem Klick auswählen.");
      return;
    }
    const onTask: Record<string, (t: TaskRef) => void> = {
      x: (t) => {
        // Auswahl rückt auf die nächste Zeile, weil die erledigte verschwindet
        const keys = visibleKeys();
        const i = keys.indexOf(selected!);
        setSelected(keys[i + 1] ?? keys[i - 1] ?? null);
        actions.complete(t);
      },
      t: (t) => actions.toggleToday(t),
      d: (t) => setDateTask(t),
      "1": (t) => actions.setPriority(t, "high"),
      "2": (t) => actions.setPriority(t, "medium"),
      "0": (t) => actions.setPriority(t, "none"),
      s: (t) => actions.toggleInProgress(t),
      o: (t) => actions.openInObsidian(t),
      f: (t) => (actions.focusEnabled ? actions.startFocus(t) : showToast("Fokus-Timer ist in den Einstellungen aus.")),
    };
    if (onTask[k]) {
      e.preventDefault();
      if (project && k === "o") {
        void openProject(project);
        return;
      }
      const t = selected && visibleKeys().includes(selected) ? registry.get(selected) : undefined;
      if (!t) showToast(project ? "Das ist ein Projekt: Enter oder o öffnet es in Obsidian." : "Erst eine Aufgabe mit j/k oder einem Klick auswählen.");
      else if (actions.readOnly && "xtd120s".includes(k)) showToast("Demo-Vault ist schreibgeschützt.");
      else onTask[k](t);
    }
  };

  // Befehlspalette: alles, was man über Tasten erreicht, auch per Suche
  const scopeLabel = { work: "Arbeit", private: "Privat", all: "Alle" } as const;
  const commands: Command[] = [
    ...VIEWS.map((v, i) => ({ id: `view:${v.id}`, group: "Ansicht", label: v.label, key: `Alt+${i + 1}`, run: () => goView(v.id) })),
    ...filters.map((f, i) => ({
      id: `filter:${f.name}`,
      group: "Filter",
      label: f.name,
      key: i < FILTER_SLOTS ? `Alt+${VIEWS.length + i + 1}` : undefined,
      run: () => goView(`filter:${f.name}` as View),
    })),
    { id: "filter-new", group: "Filter", label: "Suche / neuer Filter", key: "/", run: () => goView("filter-new") },
    ...(data?.index.projects ?? [])
      .filter((p) => p.status === "aktiv" && (scope === "all" || p.scope === scope))
      .map((p) => ({ id: `project:${p.path}`, group: "Projekt", label: `${p.name} in Obsidian öffnen`, run: () => void openProject(p.path) })),
    { id: "quick", group: "Aktion", label: "Neue Aufgabe", key: "q", run: () => setShowQuick(true) },
    { id: "reload", group: "Aktion", label: "Aktualisieren", run: () => void load() },
    ...(["work", "private", "all"] as const).map((s) => ({
      id: `scope:${s}`,
      group: "Bereich",
      label: `${scopeLabel[s]} anzeigen`,
      run: () => setScope(s),
    })),
    { id: "help", group: "Hilfe", label: "Tastenkürzel", key: "?", run: () => setShowHelp(true) },
    ...SETTINGS_TABS.map((t) => ({
      id: `settings:${t.id}`,
      group: "Einstellungen",
      label: t.label,
      key: t.id === "vault" ? "Ctrl+," : undefined,
      run: () => setShowSettings(t.id),
    })),
  ];
  useEffect(() => {
    const h = (e: KeyboardEvent) => onKeyRef.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  // Ergebnis aus dem Fenster der globalen Schnellerfassung
  const onQuickAddedRef = useRef(onQuickAdded);
  onQuickAddedRef.current = onQuickAdded;
  useEffect(() => {
    let un: (() => void) | undefined;
    let cancelled = false;
    void listen<Added>("quick-added", (ev) => onQuickAddedRef.current(ev.payload))
      .then((u) => (cancelled ? u() : (un = u)))
      .catch(() => undefined);
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);
  useEffect(() => {
    void invoke<{ shortcut: string; active: boolean; error: string | null }>("hotkey_status").then(
      (s) => {
        setHotkey(s.active ? s.shortcut : null);
        // beim Start einmal melden, wenn das globale Kürzel nicht ging (z. B. von einer anderen App belegt)
        if (s.error && configVersion === 0) showToast(`Globales Kürzel ${s.shortcut}: ${s.error} Ändern unter Einstellungen → Bedienung.`, undefined, undefined, 12000);
      },
      () => setHotkey(null),
    );
  }, [configVersion, showToast]);

  const d = data?.dashboard;
  return (
    <ActionsContext.Provider value={actions}>
    <SelectionContext.Provider value={{ selected, select: setSelected, registry }}>
    <div className="app">
      <aside>
        <div className="brand">Tasker.md</div>
        <nav>
          {VIEWS.map((v, i) => (
            <button
              key={v.id}
              className={view === v.id ? "active" : undefined}
              onClick={() => goView(v.id)}
              disabled={!v.ready}
              title={v.ready ? `Alt+${i + 1} · g ${v.g}` : "folgt"}
            >
              {v.label}
              <kbd className="nav-key">{i + 1}</kbd>
            </button>
          ))}
        </nav>
        <nav className="filters" aria-label="Eigene Filter">
          <div className="nav-title">Filter</div>
          {filters.map((f, i) => (
            <button
              key={f.name}
              className={view === `filter:${f.name}` ? "active" : undefined}
              onClick={() => goView(`filter:${f.name}`)}
              title={i < FILTER_SLOTS ? `${f.query} · Alt+${VIEWS.length + i + 1}` : f.query}
            >
              {f.name}
              {i < FILTER_SLOTS && <kbd className="nav-key">{VIEWS.length + i + 1}</kbd>}
            </button>
          ))}
          <button className={view === "filter-new" ? "active muted" : "muted"} onClick={() => setView("filter-new")}>
            + Neuer Filter
          </button>
        </nav>
        <div className="footer">
          {data && (
            <>
              <div>Inbox: {data.index.inboxCount}</div>
              <div className="muted">{data.source === "demo" ? "Demo-Vault" : data.vaultName}</div>
            </>
          )}
          {demo && (
            <button className="link" onClick={() => setDemo(false)} title="Zurück zum eigenen Vault">
              Demo beenden
            </button>
          )}
          <div className={live ? "live on" : "live"} title={live ? "Änderungen im Vault werden automatisch übernommen" : "Automatisches Neuladen nicht aktiv"}>
            {live ? "● live" : "○ nicht live"}
          </div>
          <button onClick={() => setShowQuick(true)} title="Schnelleingabe (Taste q)">
            + Aufgabe
          </button>
          <button onClick={() => void load()}>Aktualisieren</button>
          <button className="with-key" onClick={() => setShowPalette(true)} title="Befehlspalette (: oder Ctrl+K)">
            Befehle … <kbd className="nav-key">:</kbd>
          </button>
          <button onClick={() => setShowSettings("vault")} title="Einstellungen (Ctrl+,)">
            ⚙ Einstellungen
          </button>
        </div>
      </aside>
      <main>
        <header>
          <h1>{VIEWS.find((v) => v.id === view)?.label ?? (view === "filter-new" ? "Suche / neuer Filter" : activeFilter?.name)}</h1>
          <div className="segmented" role="group" aria-label="Bereich">
            {(["work", "private", "all"] as const).map((s) => (
              <button key={s} className={scope === s ? "active" : undefined} onClick={() => setScope(s)}>
                {s === "work" ? "Arbeit" : s === "private" ? "Privat" : "Alle"}
              </button>
            ))}
          </div>
        </header>
        <FocusBar f={focus} />
        {error && <div className="error">{error}</div>}
        {data && !data.workConfigured && scope !== "all" && (
          <div className="hint">
            <span>
              Keine Arbeitsbereiche eingetragen: Alle Aufgaben zählen als <b>Privat</b>, unter „Arbeit“ bleibt die
              Liste leer.
            </span>
            <button onClick={() => setShowSettings("gtd")}>Bereiche eintragen …</button>
          </div>
        )}
        {d && view === "dashboard" && (
          <div className="grid">
            {d.inProgress.length > 0 && <Section title="In Arbeit" tasks={d.inProgress} />}
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
        {(view === "filter-new" || activeFilter) && (
          <FilterView
            filter={activeFilter}
            all={filters}
            scope={scope === "all" ? null : scope}
            demo={demo}
            readOnly={data?.source === "demo"}
            onSaved={(list, open) => {
              setFilters(list);
              setView(open ? `filter:${open}` : "dashboard");
              showToast(open ? `Filter „${open}“ gespeichert` : "Filter gelöscht");
            }}
          />
        )}
        {view === "upcoming" && <UpcomingView scope={scope === "all" ? null : scope} demo={demo} />}
        {view === "today" && <TodayView scope={scope === "all" ? null : scope} demo={demo} />}
        {view === "jira" && <JiraView key={configVersion} demo={demo} />}
        {data && data.index.warnings.length > 0 && (
          <details className="warnings">
            <summary>{data.index.warnings.length} Hinweise</summary>
            <ul>{data.index.warnings.map((w) => <li key={w}>{w}</li>)}</ul>
          </details>
        )}
      </main>
      {dateTask && (
        <DateDialog
          task={dateTask}
          onClose={() => setDateTask(null)}
          onPick={(date) => {
            const t = dateTask;
            setDateTask(null);
            void actions.scheduleOn(t, date);
          }}
        />
      )}
      {showHelp && <ShortcutHelp hotkey={hotkey} onClose={() => setShowHelp(false)} />}
      {showPalette && <CommandPalette commands={commands} onClose={() => setShowPalette(false)} />}
      {showQuick && (
        <QuickEntry demo={demo} readOnly={data?.source === "demo"} onClose={() => setShowQuick(false)} onAdded={onQuickAdded} />
      )}
      {showSettings && (
        <Settings
          initialTab={showSettings}
          demo={demo}
          onDemoChange={setDemo}
          onClose={() => setShowSettings(false)}
          onSaved={() => {
            void invoke<{ active: boolean; error: string | null }>("hotkey_apply").then((s) => s.error && showToast(s.error));
            setConfigVersion((v) => v + 1);
            setVersion((v) => v + 1);
            showToast("Einstellungen gespeichert.");
          }}
        />
      )}
      {toast && (
        <div className="toast" role="status">
          <span>{toast.text}</span>
          {toast.undo && (
            <button onClick={() => undo(toast.undo!)} disabled={busy}>
              Rückgängig
            </button>
          )}
          {toast.complete && (
            <button onClick={() => (setToast(null), void completeFresh(toast.complete!))} disabled={busy}>
              Erledigt abhaken
            </button>
          )}
          {toast.undoInsert && (
            <button onClick={() => (setToast(null), void undoInsert(toast.undoInsert!))} disabled={busy}>
              Rückgängig
            </button>
          )}
          {toast.undoLog && (
            <button onClick={() => (setToast(null), void undoLog(toast.undoLog!))} disabled={busy}>
              Log rückgängig
            </button>
          )}
          <button className="close" onClick={() => setToast(null)} aria-label="Schließen">
            ✕
          </button>
        </div>
      )}
    </div>
    </SelectionContext.Provider>
    </ActionsContext.Provider>
  );
}
