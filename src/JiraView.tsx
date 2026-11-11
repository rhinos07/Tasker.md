// Jira-Ansicht (SPEC 5.4): eigene Tickets nach Status, mit Bezügen in den Vault.
// Nur lesen. Der Token wird nur eingegeben, nie angezeigt (er liegt im OS-Schlüsselbund).

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { JiraStatus, JiraView as View, LinkedIssue, NoteRef } from "./types";

const PRIO: Record<string, string> = { Highest: "🔺", High: "⏫", Medium: "🔼", Low: "🔽", Lowest: "⏬" };

function open(url: string) {
  if (url.startsWith("https://")) void openUrl(url);
}

function age(iso: string | null): string {
  if (!iso) return "noch nie";
  const min = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (min < 1) return "gerade eben";
  if (min < 60) return `vor ${min} min`;
  const h = Math.round(min / 60);
  return h < 24 ? `vor ${h} h` : new Date(iso).toLocaleString();
}

function isStale(s: JiraStatus): boolean {
  if (!s.fetchedAt || s.refreshMinutes <= 0) return !s.fetchedAt;
  return Date.now() - new Date(s.fetchedAt).getTime() > s.refreshMinutes * 60000;
}

/** Bezüge im Vault: „→ Projekt“ für Zuordnungen, „in N Notizen“ für Erwähnungen (aufklappbar). */
function VaultRefs({ assigned, mentioned }: { assigned: NoteRef[]; mentioned: NoteRef[] }) {
  const others = mentioned.filter((m) => !assigned.some((a) => a.path === m.path));
  return (
    <>
      {assigned.map((a) => (
        <span key={a.path} className="badge jira" title={a.path}>
          → {a.name}
        </span>
      ))}
      {others.length > 0 && (
        <details className="refs">
          <summary className="badge muted">in {others.length === 1 ? "1 Notiz" : `${others.length} Notizen`}</summary>
          <ul>
            {others.map((m) => (
              <li key={m.path} title={m.path}>
                {m.name}
              </li>
            ))}
          </ul>
        </details>
      )}
      {assigned.length === 0 && others.length === 0 && <span className="muted">–</span>}
    </>
  );
}

function IssueRow({ i }: { i: LinkedIssue }) {
  return (
    <tr>
      <td>
        <a href={i.url} onClick={(e) => (e.preventDefault(), open(i.url))} className="key">
          {i.key}
        </a>
      </td>
      <td>{i.summary}</td>
      <td className="muted">{i.issueType}</td>
      <td title={i.priority}>{PRIO[i.priority] ?? i.priority}</td>
      <td className="muted">{i.updated}</td>
      <td>
        <VaultRefs assigned={i.assignedTo} mentioned={i.mentionedIn} />
      </td>
    </tr>
  );
}

function Setup({ status, onSaved }: { status: JiraStatus; onSaved: () => void }) {
  const [email, setEmail] = useState(status.email);
  const [token, setToken] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [where, setWhere] = useState<string | null>(null);
  useEffect(() => {
    void invoke<string | null>("config_location").then(setWhere);
  }, []);

  const save = async () => {
    setBusy(true);
    setMsg(null);
    try {
      await invoke("jira_save_credentials", { email, token: token || null });
      setToken("");
      const user = await invoke<string>("jira_test");
      setMsg({ ok: true, text: `Verbunden als ${user}.` });
      onSaved();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  if (!status.siteSet) {
    return (
      <section className="card wide">
        <h2>Jira einrichten</h2>
        <p>
          In der Konfiguration fehlt die Jira-Adresse. Bitte <code>jira.baseUrl</code> eintragen, z. B.{" "}
          <code>https://&lt;firma&gt;.atlassian.net</code>.
        </p>
        {where && <p className="muted">Konfiguration: {where}</p>}
      </section>
    );
  }
  return (
    <section className="card wide setup">
      <h2>Jira verbinden</h2>
      <p className="muted">
        Anmeldung mit E-Mail und API-Token. Der Token wird im Schlüsselbund des Betriebssystems gespeichert und nicht
        wieder angezeigt.
        {status.tokenType !== "classic" && " Empfohlene Bereiche: read:jira-work, read:jira-user."}
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <label>
          E-Mail
          <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} autoComplete="username" required />
        </label>
        <label>
          API-Token {status.hasToken && <span className="muted">(gespeichert, leer lassen zum Behalten)</span>}
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            autoComplete="off"
            required={!status.hasToken}
          />
        </label>
        <button type="submit" disabled={busy}>
          {busy ? "Prüfe …" : "Speichern & testen"}
        </button>
      </form>
      {msg && <p className={msg.ok ? "ok" : "error"}>{msg.text}</p>}
    </section>
  );
}

export default function JiraView({ demo }: { demo: boolean }) {
  const [status, setStatus] = useState<JiraStatus | null>(null);
  const [view, setView] = useState<View | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showSetup, setShowSetup] = useState(false);

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      setView(await invoke<View>("jira_refresh", { demo }));
      setStatus(await invoke<JiraStatus>("jira_status", { demo }));
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [demo]);

  const init = useCallback(async () => {
    try {
      const s = await invoke<JiraStatus>("jira_status", { demo });
      setStatus(s);
      if (!s.configured || !s.hasToken) return;
      setView(await invoke<View>("jira_issues", { demo }));
      if (isStale(s)) await refresh();
    } catch (e) {
      setError(String(e));
    }
  }, [demo, refresh]);

  useEffect(() => {
    void init();
  }, [init]);

  // regelmäßig aktualisieren, solange die Ansicht offen ist
  useEffect(() => {
    if (!status || status.demo || !status.hasToken || status.refreshMinutes <= 0) return;
    const id = window.setInterval(() => void refresh(), status.refreshMinutes * 60000);
    return () => window.clearInterval(id);
  }, [status?.demo, status?.hasToken, status?.refreshMinutes, refresh]);

  // Vault geändert → Bezüge neu verknüpfen (ohne Netzwerk)
  const demoRef = useRef(demo);
  demoRef.current = demo;
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("vault-changed", () => {
      void invoke<View>("jira_issues", { demo: demoRef.current }).then(setView, () => undefined);
    }).then((u) => (cancelled ? u() : (unlisten = u)));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const disconnect = async () => {
    try {
      await invoke("jira_delete_token");
      setView(null);
      await init();
    } catch (e) {
      setError(String(e));
    }
  };

  if (!status) return error ? <div className="error">{error}</div> : null;
  const needsSetup = !status.configured || !status.hasToken;

  return (
    <>
      {(needsSetup || showSetup) && !status.demo && (
        <Setup
          status={status}
          onSaved={() => {
            setShowSetup(false);
            void init();
          }}
        />
      )}
      {!needsSetup && (
        <div className="jira-bar">
          <span>
            {status.demo ? "Demo-Daten" : <>Verbunden als {view?.user ?? status.user ?? "?"}</>} · Stand{" "}
            {age(view?.fetchedAt ?? status.fetchedAt)} · {view?.total ?? 0} Tickets
          </span>
          <button onClick={() => void refresh()} disabled={busy}>
            {busy ? "Lade …" : "Aktualisieren"}
          </button>
          {!status.demo && (
            <>
              <button onClick={() => setShowSetup((x) => !x)}>Zugang …</button>
              <button onClick={() => void disconnect()} title="Token aus dem Schlüsselbund entfernen">
                Trennen
              </button>
            </>
          )}
        </div>
      )}
      {error && <div className="error">{error}</div>}
      {view && !needsSetup && (
        <>
          {view.groups.length === 0 && (
            <section className="card wide">
              <p className="empty">Keine Tickets für die Abfrage.</p>
            </section>
          )}
          {view.groups.map((g) => (
            <section key={g.status} className="card wide">
              <h2>
                {g.status} <span className="count">{g.issues.length}</span>
              </h2>
              <table className="jira">
                <thead>
                  <tr>
                    <th>Ticket</th>
                    <th>Titel</th>
                    <th>Typ</th>
                    <th>Prio</th>
                    <th>Geändert</th>
                    <th>Im Vault</th>
                  </tr>
                </thead>
                <tbody>
                  {g.issues.map((i) => (
                    <IssueRow key={i.key} i={i} />
                  ))}
                </tbody>
              </table>
            </section>
          ))}
          {view.vaultOnly.length > 0 && (
            <details className="card wide vault-only">
              <summary>
                <h2>
                  Nur im Vault <span className="count">{view.vaultOnly.length}</span>
                </h2>
                <span className="muted">Tickets mit Bezug im Vault, die nicht in deiner Abfrage sind</span>
              </summary>
              <table className="jira">
                <tbody>
                  {view.vaultOnly.map((v) => (
                    <tr key={v.key}>
                      <td>
                        <a href={v.url} onClick={(e) => (e.preventDefault(), open(v.url))} className="key">
                          {v.key}
                        </a>
                      </td>
                      <td>
                        <VaultRefs assigned={v.assignedTo} mentioned={v.mentionedIn} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </details>
          )}
        </>
      )}
    </>
  );
}
