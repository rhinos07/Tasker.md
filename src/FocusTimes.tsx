// „Zeiten heute“ (SPEC 6.5): Fokus-Zeiten pro Ticket zum Abschreiben in Jira. Die App bucht nichts.

import { useCallback, useContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ActionsContext } from "./tasks";

interface TaskTime {
  taskKey: string;
  description: string;
  path: string;
  minutes: number;
  sessions: number;
}

interface DayRow {
  key: string | null;
  source: "line" | "project" | "chosen" | "guess" | "multi" | "none";
  options: string[];
  rawMinutes: number;
  roundedMinutes: number;
  sessions: number;
  firstStart: string;
  tasks: TaskTime[];
  booked: boolean;
  rowId: string;
  title: string | null;
  url: string | null;
  copyText: string;
}

interface DaySummary {
  date: string;
  rows: DayRow[];
  totalMinutes: number;
  roundToMinutes: number;
  knownTickets: [string, string][];
}

const hm = (m: number) => `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;

const SOURCE: Record<DayRow["source"], string> = {
  line: "aus der Aufgabenzeile",
  project: "Ticket des Projekts",
  chosen: "von dir zugeordnet",
  guess: "vermutet: einziges im Projekt erwähntes Ticket",
  multi: "Projekt hat mehrere Tickets",
  none: "kein Ticket gefunden",
};

const localDate = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

export default function FocusTimes({ demo }: { demo: boolean }) {
  const a = useContext(ActionsContext)!;
  const [date, setDate] = useState(localDate());
  const [data, setData] = useState<DaySummary | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setData(await invoke<DaySummary>("focus_day", { date, demo }));
    } catch (e) {
      setMsg(String(e));
    }
  }, [date, demo]);

  useEffect(() => {
    void load();
  }, [load, a.version]);

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setMsg("Kopiert.");
    } catch {
      setMsg("Kopieren nicht möglich.");
    }
  };

  const choose = async (r: DayRow, ticket: string) => {
    const taskKey = r.tasks[0].taskKey;
    await invoke("focus_choose_ticket", { taskKey, ticket: ticket || null, demo }).catch((e) => setMsg(String(e)));
    await load();
  };

  const setBooked = async (r: DayRow, booked: boolean) => {
    await invoke("focus_set_booked", { date, rowId: r.rowId, booked, demo }).catch((e) => setMsg(String(e)));
    await load();
  };

  const rows = data?.rows ?? [];
  const withKey = rows.filter((r) => r.key);
  const open = rows.filter((r) => !r.booked);

  return (
    <section className="card wide focus-times">
      <h2>
        Fokus-Zeiten{" "}
        <input type="date" value={date} onChange={(e) => e.target.value && setDate(e.target.value)} />
        {data && <span className="count"> {hm(data.totalMinutes)} gesamt</span>}
      </h2>
      <p className="muted small">
        Fokus-Zeit erfasst nur konzentrierte Blöcke, Meetings und Abstimmungen fehlen. Die Werte sind also nicht der
        volle Aufwand.
        {data && data.roundToMinutes > 0 && ` Gerundet auf ${data.roundToMinutes} min pro Ticket und Tag.`}
      </p>
      {rows.length === 0 ? (
        <p className="empty">Keine Fokus-Sitzungen an diesem Tag.</p>
      ) : (
        <>
          <table className="jira">
            <thead>
              <tr>
                <th>Ticket</th>
                <th>Zeit</th>
                <th>Aufgaben</th>
                <th>ab</th>
                <th>gebucht</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.rowId} className={r.booked ? "booked" : undefined}>
                  <td>
                    {r.key ? (
                      <>
                        <span className="key" title={SOURCE[r.source]}>
                          {r.key}
                          {r.source === "guess" && " ?"}
                        </span>
                        {r.title && <div className="muted small">{r.title}</div>}
                        {r.source === "chosen" && (
                          <button className="link" onClick={() => void choose(r, "")} title="Zuordnung entfernen">
                            Zuordnung lösen
                          </button>
                        )}
                      </>
                    ) : (
                      <select value="" onChange={(e) => void choose(r, e.target.value)} title={SOURCE[r.source]}>
                        <option value="">{r.source === "multi" ? "Ticket wählen …" : "ohne Ticket – zuordnen …"}</option>
                        {(r.options.length > 0 ? r.options.map((k) => [k, ""] as [string, string]) : (data?.knownTickets ?? [])).map(
                          ([k, t]) => (
                            <option key={k} value={k}>
                              {k}
                              {t && ` · ${t.slice(0, 50)}`}
                            </option>
                          ),
                        )}
                      </select>
                    )}
                  </td>
                  <td>
                    <b>{hm(r.roundedMinutes)}</b>
                    {r.roundedMinutes !== r.rawMinutes && <div className="muted small">roh {r.rawMinutes} min</div>}
                    <div className="muted small">
                      {r.sessions} {r.sessions === 1 ? "Sitzung" : "Sitzungen"}
                    </div>
                  </td>
                  <td>
                    {r.tasks.map((t) => (
                      <div key={t.taskKey} title={t.path}>
                        {t.description} <span className="muted">({t.minutes} min)</span>
                      </div>
                    ))}
                  </td>
                  <td className="muted">{r.firstStart}</td>
                  <td>
                    <input type="checkbox" checked={r.booked} onChange={(e) => void setBooked(r, e.target.checked)} />
                  </td>
                  <td className="actions">
                    <button onClick={() => void copy(r.copyText)}>Kopieren</button>
                    {r.url && <button onClick={() => void openUrl(r.url!)}>In Jira öffnen</button>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="row">
            <button
              onClick={() => void copy(open.filter((r) => r.key).map((r) => r.copyText).join("\n"))}
              disabled={withKey.filter((r) => !r.booked).length === 0}
            >
              Alles Offene kopieren
            </button>
            {msg && <span className="muted small">{msg}</span>}
          </div>
        </>
      )}
    </section>
  );
}
