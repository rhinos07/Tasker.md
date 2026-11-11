// Einstellungsdialog (SPEC 11): bearbeitet config.json im App-Datenordner.
// Zugangsdaten (Jira-Token, Google) werden hier nicht angezeigt; die liegen im Schlüsselbund
// und werden in den Ansichten Jira bzw. Heute → Kalender verwaltet.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface Config {
  vault: { path: string; name: string };
  folders: { inbox: string; projects: string; areas: string; archive: string; journal: string; templates: string };
  scan: { exclude: string[] };
  projects: { reviewAfterDays: number };
  areas: { work: string[]; workPaths: string[] };
  contexts: string[];
  waitingTag: string;
  timeline: { start: string; end: string; defaultBlockMinutes: number };
  focus: {
    enabled: boolean;
    pomodoro: { work: number; break: number };
    writeToVault: boolean;
    logSection: string;
    logFormat: string;
    minMinutes: number;
  };
  jira: {
    baseUrl: string;
    jql: string;
    projectKeys: string[];
    refreshMinutes: number;
    tokenType: string;
    worklog: { roundToMinutes: number };
  };
  calendar: { refreshMinutes: number; daysAhead: number };
  hotkeys: { quickCapture: string };
  obsidian: { useAdvancedUri: string };
}

interface SettingsData {
  path: string | null;
  exists: boolean;
  config: Config;
}

type Tab = "vault" | "gtd" | "focus" | "jira" | "calendar" | "keys";
const TABS: { id: Tab; label: string }[] = [
  { id: "vault", label: "Vault" },
  { id: "gtd", label: "GTD" },
  { id: "focus", label: "Heute & Fokus" },
  { id: "jira", label: "Jira" },
  { id: "calendar", label: "Kalender" },
  { id: "keys", label: "Bedienung" },
];

const lines = (v: string[]) => v.join("\n");
const unlines = (s: string) =>
  s
    .split("\n")
    .map((x) => x.trim())
    .filter(Boolean);

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <small className="muted">{hint}</small>}
    </label>
  );
}

export default function Settings({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [data, setData] = useState<SettingsData | null>(null);
  const [c, setC] = useState<Config | null>(null);
  // Listen als Text, damit beim Tippen keine Zeilen verschwinden
  const [lists, setLists] = useState<Record<string, string>>({});
  const [tab, setTab] = useState<Tab>("vault");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void invoke<SettingsData>("get_settings").then(
      (d) => {
        setData(d);
        setC(d.config);
        setLists({
          exclude: lines(d.config.scan.exclude),
          work: lines(d.config.areas.work),
          workPaths: lines(d.config.areas.workPaths),
          contexts: lines(d.config.contexts),
          projectKeys: lines(d.config.jira.projectKeys),
        });
      },
      (e) => setError(String(e)),
    );
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  if (!c || !data) {
    return (
      <div className="modal-backdrop" onClick={onClose}>
        <div className="modal" onClick={(e) => e.stopPropagation()}>
          {error ? <div className="error">{error}</div> : <p className="muted">Lade …</p>}
        </div>
      </div>
    );
  }

  // kleine Helfer zum Ändern verschachtelter Werte
  const set = <K extends keyof Config>(k: K, v: Partial<Config[K]>) => setC({ ...c, [k]: { ...(c[k] as object), ...v } } as Config);
  const num = (s: string) => (s === "" ? 0 : Number(s));
  const list = (k: string) => (
    <textarea rows={4} value={lists[k] ?? ""} onChange={(e) => setLists({ ...lists, [k]: e.target.value })} />
  );

  const pickVault = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Vault-Ordner wählen" });
    if (typeof dir === "string") {
      const name = dir.split(/[\\/]/).filter(Boolean).pop() ?? "";
      set("vault", { path: dir, name: c.vault.name || name });
    }
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    const patch = {
      vault: c.vault,
      folders: c.folders,
      scan: { exclude: unlines(lists.exclude) },
      projects: c.projects,
      areas: { work: unlines(lists.work), workPaths: unlines(lists.workPaths) },
      contexts: unlines(lists.contexts),
      waitingTag: c.waitingTag,
      timeline: c.timeline,
      focus: c.focus,
      jira: {
        baseUrl: c.jira.baseUrl.trim(),
        jql: c.jira.jql,
        projectKeys: unlines(lists.projectKeys).map((k) => k.toUpperCase()),
        refreshMinutes: c.jira.refreshMinutes,
        tokenType: c.jira.tokenType,
        worklog: { roundToMinutes: c.jira.worklog.roundToMinutes },
      },
      calendar: { refreshMinutes: c.calendar.refreshMinutes, daysAhead: c.calendar.daysAhead },
      hotkeys: { quickCapture: c.hotkeys.quickCapture.trim() },
      obsidian: c.obsidian,
    };
    try {
      await invoke("save_settings", { patch });
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal settings" role="dialog" aria-label="Einstellungen" onClick={(e) => e.stopPropagation()}>
        <header>
          <h2>{data.exists ? "Einstellungen" : "Einrichtung"}</h2>
          <button className="close" onClick={onClose} aria-label="Schließen">
            ✕
          </button>
        </header>
        {!data.exists && (
          <p className="muted small">Noch keine Konfiguration vorhanden. Bitte mindestens den Vault-Ordner wählen und speichern.</p>
        )}
        <nav className="tabs">
          {TABS.map((t) => (
            <button key={t.id} className={tab === t.id ? "active" : undefined} onClick={() => setTab(t.id)}>
              {t.label}
            </button>
          ))}
        </nav>
        <div className="settings-body">
          {tab === "vault" && (
            <>
              <Field label="Vault-Ordner">
                <div className="row">
                  <input value={c.vault.path} onChange={(e) => set("vault", { path: e.target.value })} />
                  <button onClick={() => void pickVault()}>Wählen …</button>
                </div>
              </Field>
              <Field label="Vault-Name" hint="Name des Vaults in Obsidian, für „In Obsidian öffnen“">
                <input value={c.vault.name} onChange={(e) => set("vault", { name: e.target.value })} />
              </Field>
              <div className="grid2">
                {(
                  [
                    ["inbox", "Inbox"],
                    ["projects", "Projekte"],
                    ["areas", "Bereiche"],
                    ["archive", "Archiv"],
                    ["journal", "Tagesnotizen"],
                    ["templates", "Vorlagen"],
                  ] as const
                ).map(([k, label]) => (
                  <Field key={k} label={`Ordner ${label}`}>
                    <input value={c.folders[k]} onChange={(e) => set("folders", { [k]: e.target.value })} />
                  </Field>
                ))}
              </div>
              <Field label="Nicht scannen" hint="Glob-Muster, eines pro Zeile, z. B. 30 Wissen/**">
                {list("exclude")}
              </Field>
            </>
          )}
          {tab === "gtd" && (
            <>
              <Field label="Bereiche, die als „Arbeit“ zählen" hint="Wert im Frontmatter-Feld bereich, einer pro Zeile">
                {list("work")}
              </Field>
              <Field label="Pfade, die als „Arbeit“ zählen" hint="Pfad-Anfang relativ zum Vault, einer pro Zeile">
                {list("workPaths")}
              </Field>
              <Field label="Kontexte" hint="z. B. #@computer, einer pro Zeile">
                {list("contexts")}
              </Field>
              <div className="grid2">
                <Field label="Warten-Tag">
                  <input value={c.waitingTag} onChange={(e) => setC({ ...c, waitingTag: e.target.value })} />
                </Field>
                <Field label="Review fällig nach (Tagen)">
                  <input type="number" min={1} value={c.projects.reviewAfterDays} onChange={(e) => set("projects", { reviewAfterDays: num(e.target.value) })} />
                </Field>
              </div>
            </>
          )}
          {tab === "focus" && (
            <>
              <h3>Zeitleiste</h3>
              <div className="grid3">
                <Field label="Beginn">
                  <input type="time" value={c.timeline.start} onChange={(e) => set("timeline", { start: e.target.value })} />
                </Field>
                <Field label="Ende">
                  <input type="time" value={c.timeline.end} onChange={(e) => set("timeline", { end: e.target.value })} />
                </Field>
                <Field label="Blocklänge (min)">
                  <input type="number" min={5} step={5} value={c.timeline.defaultBlockMinutes} onChange={(e) => set("timeline", { defaultBlockMinutes: num(e.target.value) })} />
                </Field>
              </div>
              <h3>Fokus-Timer</h3>
              <label className="check-row">
                <input type="checkbox" checked={c.focus.enabled} onChange={(e) => set("focus", { enabled: e.target.checked })} /> Fokus-Timer anzeigen
              </label>
              <div className="grid3">
                <Field label="Arbeit (min)">
                  <input type="number" min={1} value={c.focus.pomodoro.work} onChange={(e) => set("focus", { pomodoro: { ...c.focus.pomodoro, work: num(e.target.value) } })} />
                </Field>
                <Field label="Pause (min)">
                  <input type="number" min={0} value={c.focus.pomodoro.break} onChange={(e) => set("focus", { pomodoro: { ...c.focus.pomodoro, break: num(e.target.value) } })} />
                </Field>
                <Field label="Mindestdauer fürs Log (min)">
                  <input type="number" min={0} value={c.focus.minMinutes} onChange={(e) => set("focus", { minMinutes: num(e.target.value) })} />
                </Field>
              </div>
              <label className="check-row">
                <input type="checkbox" checked={c.focus.writeToVault} onChange={(e) => set("focus", { writeToVault: e.target.checked })} /> Fokus-Zeiten ins Log der Projekt- bzw. Bereichsnotiz schreiben
              </label>
              <div className="grid2">
                <Field label="Log-Überschrift">
                  <input value={c.focus.logSection} onChange={(e) => set("focus", { logSection: e.target.value })} />
                </Field>
                <Field label="Log-Format" hint="Platzhalter {date} {minutes} {sessions} {task}">
                  <input value={c.focus.logFormat} onChange={(e) => set("focus", { logFormat: e.target.value })} />
                </Field>
              </div>
            </>
          )}
          {tab === "jira" && (
            <>
              <Field label="Jira-Adresse" hint="z. B. https://<firma>.atlassian.net, leer = Jira aus. E-Mail und Token unter Jira → Zugang.">
                <input value={c.jira.baseUrl} placeholder="https://<firma>.atlassian.net" onChange={(e) => set("jira", { baseUrl: e.target.value })} />
              </Field>
              <Field label="Token-Typ">
                <select value={c.jira.tokenType} onChange={(e) => set("jira", { tokenType: e.target.value })}>
                  <option value="scoped">mit Bereichen (empfohlen)</option>
                  <option value="classic">klassisch</option>
                </select>
              </Field>
              <Field label="JQL-Abfrage">
                <textarea rows={3} value={c.jira.jql} onChange={(e) => set("jira", { jql: e.target.value })} />
              </Field>
              <Field label="Projekt-Schlüssel" hint="Präfixe, die im Vault als Ticket erkannt werden, z. B. ABC, einer pro Zeile">
                {list("projectKeys")}
              </Field>
              <div className="grid2">
                <Field label="Aktualisieren alle (min)">
                  <input type="number" min={1} value={c.jira.refreshMinutes} onChange={(e) => set("jira", { refreshMinutes: num(e.target.value) })} />
                </Field>
                <Field label="Fokus-Zeiten runden auf (min)" hint="0 = nicht runden">
                  <input type="number" min={0} value={c.jira.worklog.roundToMinutes} onChange={(e) => set("jira", { worklog: { ...c.jira.worklog, roundToMinutes: num(e.target.value) } })} />
                </Field>
              </div>
            </>
          )}
          {tab === "keys" && (
            <>
              <h3>Globales Tastenkürzel</h3>
              <Field
                label="Schnellerfassung"
                hint="Öffnet die Schnellerfassung auch, wenn die App im Hintergrund ist. Beispiele: CmdOrCtrl+Alt+Space, Ctrl+Shift+T. Leer = aus. Die App muss laufen."
              >
                <input value={c.hotkeys.quickCapture} onChange={(e) => set("hotkeys", { quickCapture: e.target.value })} />
              </Field>
              <h3>In Obsidian öffnen</h3>
              <Field
                label="Zeilensprung über das Plugin „Advanced URI“"
                hint="Ist das Plugin im Vault aktiv, springt „o“ direkt auf die Zeile. Sonst öffnet die App die Notiz und legt den Aufgabentext in die Zwischenablage. Die App liest .obsidian/ nur."
              >
                <select value={c.obsidian.useAdvancedUri} onChange={(e) => set("obsidian", { useAdvancedUri: e.target.value })}>
                  <option value="auto">verwenden, wenn installiert</option>
                  <option value="never">nie verwenden</option>
                </select>
              </Field>
              <p className="muted small">Alle Tastenkürzel in der App zeigt die Taste „?“.</p>
            </>
          )}
          {tab === "calendar" && (
            <>
              <p className="muted small">Verbindung und Auswahl der Kalender unter Heute → Kalender.</p>
              <div className="grid2">
                <Field label="Aktualisieren alle (min)">
                  <input type="number" min={1} value={c.calendar.refreshMinutes} onChange={(e) => set("calendar", { refreshMinutes: num(e.target.value) })} />
                </Field>
                <Field label="Tage im Voraus laden">
                  <input type="number" min={1} max={60} value={c.calendar.daysAhead} onChange={(e) => set("calendar", { daysAhead: num(e.target.value) })} />
                </Field>
              </div>
            </>
          )}
        </div>
        {error && <div className="error">{error}</div>}
        <footer>
          {data.path && <span className="muted small path" title={data.path}>{data.path}</span>}
          <button onClick={onClose}>Abbrechen</button>
          <button className="primary" onClick={() => void save()} disabled={busy}>
            {busy ? "Speichere …" : "Speichern"}
          </button>
        </footer>
      </div>
    </div>
  );
}
