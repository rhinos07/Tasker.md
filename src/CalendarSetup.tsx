// Einrichtung der Kalender (SPEC 7): Google (eigener OAuth-Client, Anmeldung im Browser, Kalenderauswahl)
// und abonnierte ICS-Links, z. B. Outlook „Kalender veröffentlichen“.
// Client-Secret, Tokens und ICS-Links werden nie angezeigt; sie liegen im Schlüsselbund des Betriebssystems.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface CalendarStatus {
  clientSet: boolean;
  connected: boolean;
  calendarIds: string[];
  ics: { id: string; name: string }[];
  active: boolean;
  refreshMinutes: number;
  fetchedAt: string | null;
}

interface Choice {
  id: string;
  summary: string;
  primary: boolean;
  selected: boolean;
}

export default function CalendarSetup({ status, onChanged }: { status: CalendarStatus; onChanged: () => void }) {
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [choices, setChoices] = useState<Choice[] | null>(null);
  const [icsName, setIcsName] = useState("");
  const [icsUrl, setIcsUrl] = useState("");

  const act = async (fn: () => Promise<void>) => {
    setBusy(true);
    setMsg(null);
    try {
      await fn();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (status.connected) void act(async () => setChoices(await invoke<Choice[]>("calendar_list")));
  }, [status.connected]);

  const saveClient = () =>
    act(async () => {
      await invoke("calendar_save_client", { clientId, clientSecret: secret });
      setClientId("");
      setSecret("");
      setMsg({ ok: true, text: "Client gespeichert. Jetzt mit Google verbinden." });
      onChanged();
    });

  const connect = () =>
    act(async () => {
      setMsg({ ok: true, text: "Browser geöffnet. Bitte dort anmelden und den Lesezugriff erlauben …" });
      await invoke("calendar_connect");
      await invoke("calendar_refresh");
      setMsg({ ok: true, text: "Verbunden." });
      onChanged();
    });

  const saveSelection = () =>
    act(async () => {
      const ids = (choices ?? []).filter((c) => c.selected).map((c) => (c.primary ? "primary" : c.id));
      await invoke("calendar_select", { ids });
      const n = await invoke<number>("calendar_refresh");
      setMsg({ ok: true, text: `Gespeichert, ${n} Termine geladen.` });
      onChanged();
    });

  const disconnect = (forgetClient: boolean) =>
    act(async () => {
      await invoke("calendar_disconnect", { forgetClient });
      setChoices(null);
      onChanged();
    });

  const addIcs = () =>
    act(async () => {
      setMsg({ ok: true, text: "Prüfe den Link …" });
      await invoke("calendar_add_ics", { name: icsName, url: icsUrl });
      setIcsName("");
      setIcsUrl("");
      const n = await invoke<number>("calendar_refresh");
      setMsg({ ok: true, text: `Abonniert, ${n} Termine geladen.` });
      onChanged();
    });

  const removeIcs = (id: string, name: string) => {
    if (!window.confirm(`Kalender „${name}“ entfernen? Der Link wird aus dem Schlüsselbund gelöscht.`)) return;
    void act(async () => {
      await invoke("calendar_remove_ics", { id });
      onChanged();
    });
  };

  return (
    <div className="calendar-setup">
      <h4>Google Kalender</h4>
      {!status.clientSet && (
        <>
          <p className="muted small">
            Einmalig: In der Google Cloud Console ein Projekt anlegen, die Google Calendar API aktivieren und einen
            OAuth-Client vom Typ „Desktop-App“ erstellen. Die Anleitung steht in der README.
          </p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void saveClient();
            }}
          >
            <label>
              Client-ID
              <input value={clientId} onChange={(e) => setClientId(e.target.value)} autoComplete="off" required />
            </label>
            <label>
              Client-Secret
              <input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} autoComplete="off" required />
            </label>
            <button type="submit" disabled={busy}>
              Speichern
            </button>
          </form>
        </>
      )}
      {status.clientSet && !status.connected && (
        <div className="row">
          <button onClick={() => void connect()} disabled={busy}>
            {busy ? "Warte auf Anmeldung …" : "Mit Google verbinden"}
          </button>
          <button onClick={() => void disconnect(true)} disabled={busy} title="Client-ID und -Secret entfernen">
            Client entfernen
          </button>
        </div>
      )}
      {status.connected && (
        <>
          <p className="muted small">Angezeigte Kalender:</p>
          {choices === null ? (
            <p className="muted small">Lade Kalender …</p>
          ) : (
            <ul className="choices">
              {choices.map((c, i) => (
                <li key={c.id}>
                  <label>
                    <input
                      type="checkbox"
                      checked={c.selected}
                      onChange={(e) => setChoices(choices.map((x, j) => (j === i ? { ...x, selected: e.target.checked } : x)))}
                    />{" "}
                    {c.summary}
                    {c.primary && <span className="muted"> (Hauptkalender)</span>}
                  </label>
                </li>
              ))}
            </ul>
          )}
          <div className="row">
            <button onClick={() => void saveSelection()} disabled={busy || choices === null}>
              Auswahl speichern
            </button>
            <button onClick={() => void disconnect(false)} disabled={busy}>
              Trennen
            </button>
          </div>
        </>
      )}
      <h4>Kalender per Link (ICS)</h4>
      <p className="muted small">
        Zum Beispiel den Firmenkalender aus Outlook: Outlook im Web → Einstellungen → Kalender → Freigegebene Kalender →
        Kalender veröffentlichen, dort den ICS-Link kopieren. Der Link wird nur im Schlüsselbund gespeichert.
      </p>
      {status.ics.length > 0 && (
        <ul className="choices">
          {status.ics.map((f) => (
            <li key={f.id}>
              {f.name}{" "}
              <button className="link" onClick={() => removeIcs(f.id, f.name)} disabled={busy}>
                entfernen
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void addIcs();
        }}
      >
        <label>
          Name
          <input value={icsName} onChange={(e) => setIcsName(e.target.value)} placeholder="z. B. Firma" autoComplete="off" required />
        </label>
        <label>
          ICS-Link
          <input
            type="password"
            value={icsUrl}
            onChange={(e) => setIcsUrl(e.target.value)}
            placeholder="https://…/calendar.ics"
            autoComplete="off"
            required
          />
        </label>
        <button type="submit" disabled={busy}>
          {busy ? "Prüfe …" : "Abonnieren"}
        </button>
      </form>
      {msg && <p className={msg.ok ? "ok small" : "error"}>{msg.text}</p>}
    </div>
  );
}
