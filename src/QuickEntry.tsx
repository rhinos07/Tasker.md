// Schnelleingabe (SPEC 6.2): Eingabe in natürlicher Sprache, Vorschau der erzeugten Zeile und der Zieldatei.
// Enter speichert, Umschalt+Enter speichert und lässt das Fenster offen, Tab übernimmt einen Vorschlag.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Target {
  path: string;
  name: string;
  kind: "project" | "area";
  heading: string;
}

interface BlockPlan {
  date: string;
  start: string;
  end: string;
  minutes: number;
}

interface Preview {
  line: string;
  block: BlockPlan | null;
  target: Target | null;
  inboxPath: string | null;
  candidates: Target[];
  contexts: string[];
  problem: string | null;
}

export interface Added {
  path: string;
  line: number;
  new: string;
  addedHeading: boolean;
  createdFile: boolean;
  targetName: string;
  /** angelegter Zeitblock */
  block: { id: string; date: string; start: string; minutes: number } | null;
  /** Aufgabe angelegt, Zeitblock nicht */
  blockError: string | null;
}

/** „heute“, „morgen“ oder TT.MM. für den Tag eines Zeitblocks. */
export function blockDay(date: string): string {
  const d = new Date(`${date}T00:00:00`);
  const t = new Date();
  t.setHours(0, 0, 0, 0);
  const diff = Math.round((d.getTime() - t.getTime()) / 86400000);
  if (diff === 0) return "heute";
  if (diff === 1) return "morgen";
  return `${String(d.getDate()).padStart(2, "0")}.${String(d.getMonth() + 1).padStart(2, "0")}.`;
}

type Suggestion = { label: string; hint: string; apply: (input: string) => string };

/** Vorschläge zum gerade getippten Teil: `>Ziel` oder `@kontext`. */
function suggestionsFor(input: string, p: Preview | null): Suggestion[] {
  if (!p) return [];
  const gt = input.lastIndexOf(">");
  if (gt >= 0) {
    const typed = input.slice(gt + 1).trim();
    if (p.target && p.target.name === typed) return [];
    return p.candidates.map((t) => ({
      label: t.name,
      hint: t.kind === "project" ? "Projekt" : "Bereich",
      apply: (s) => `${s.slice(0, s.lastIndexOf(">") + 1)}${t.name}`,
    }));
  }
  const m = /(?:^|\s)@([^\s]*)$/.exec(input);
  if (m) {
    const typed = m[1].toLowerCase();
    return p.contexts
      .map((c) => c.replace(/^#?@/, ""))
      .filter((c) => c.toLowerCase().startsWith(typed) && c.toLowerCase() !== typed)
      .slice(0, 8)
      .map((c) => ({ label: `@${c}`, hint: "Kontext", apply: (s) => `${s.slice(0, s.length - typed.length)}${c} ` }));
  }
  return [];
}

export default function QuickEntry({
  demo,
  readOnly,
  onClose,
  onAdded,
}: {
  demo: boolean;
  readOnly: boolean;
  onClose: () => void;
  onAdded: (a: Added) => void;
}) {
  const [input, setInput] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const [sel, setSel] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => ref.current?.focus(), []);

  useEffect(() => {
    if (!input.trim()) {
      setPreview(null);
      return;
    }
    const id = window.setTimeout(() => {
      void invoke<Preview>("quick_preview", { input, demo }).then(
        (p) => {
          setPreview(p);
          setError(null);
        },
        (e) => setError(String(e)),
      );
    }, 120);
    return () => window.clearTimeout(id);
  }, [input, demo]);

  const sugg = suggestionsFor(input, preview);
  useEffect(() => setSel(0), [sugg.length]);

  const save = async (keepOpen: boolean) => {
    if (!preview || preview.problem || busy || readOnly) return;
    setBusy(true);
    try {
      const a = await invoke<Added>("quick_add", { input, demo });
      onAdded(a);
      if (keepOpen) {
        setInput("");
        setPreview(null);
        ref.current?.focus();
      } else onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
    else if (e.key === "ArrowDown" && sugg.length) {
      e.preventDefault();
      setSel((sel + 1) % sugg.length);
    } else if (e.key === "ArrowUp" && sugg.length) {
      e.preventDefault();
      setSel((sel - 1 + sugg.length) % sugg.length);
    } else if (e.key === "Tab" && sugg.length) {
      e.preventDefault();
      setInput(sugg[sel].apply(input));
    } else if (e.key === "Enter") {
      e.preventDefault();
      void save(e.shiftKey);
    }
  };

  return (
    <div className="modal-backdrop quick-backdrop" onClick={onClose}>
      <div className="modal quick" role="dialog" aria-label="Neue Aufgabe" onClick={(e) => e.stopPropagation()}>
        <input
          ref={ref}
          className="quick-input"
          value={input}
          placeholder="z. B. Müller anrufen morgen @telefon !1 >Projekt"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKey}
          spellCheck={false}
        />
        {sugg.length > 0 && (
          <ul className="quick-sugg">
            {sugg.map((s, i) => (
              <li
                key={s.label}
                className={i === sel ? "active" : undefined}
                onMouseDown={(e) => {
                  e.preventDefault();
                  setInput(s.apply(input));
                  ref.current?.focus();
                }}
              >
                {s.label} <span className="muted">{s.hint}</span>
              </li>
            ))}
          </ul>
        )}
        {preview && (
          <div className="quick-preview">
            <code>{preview.line}</code>
            <div className="muted small">
              {preview.target ? (
                <>
                  → {preview.target.kind === "project" ? "Projekt" : "Bereich"} <b>{preview.target.name}</b>, unter „
                  {preview.target.heading.replace(/^#+\s*/, "")}“
                </>
              ) : (
                preview.inboxPath && <>→ neue Notiz {preview.inboxPath}</>
              )}
            </div>
            {preview.block && (
              <div className="muted small">
                ⏱ Zeitblock {blockDay(preview.block.date)} {preview.block.start}–{preview.block.end} ({preview.block.minutes} min, nur lokal)
              </div>
            )}
            {preview.problem && <div className="warn-text small">{preview.problem}</div>}
          </div>
        )}
        {error && <div className="error">{error}</div>}
        {readOnly && <div className="muted small">Demo-Vault ist schreibgeschützt, nur Vorschau.</div>}
        <div className="quick-help muted small">
          heute · morgen · freitag · in 3 tagen · 24.12. · <b>fällig</b> … → 📅 · !1 ⏫ !2 🔼 · @kontext · warten · jeden montag · täglich · 14:00 90m · 14-15:30 → Zeitblock ·{" "}
          <b>&gt;Ziel</b> am Ende · Enter speichern, Umschalt+Enter weitere, Esc schließen
        </div>
      </div>
    </div>
  );
}
