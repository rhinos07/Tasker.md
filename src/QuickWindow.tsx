// Kleines Fenster der Schnellerfassung, geöffnet über das globale Tastenkürzel (SPEC 6.6).
// Nach dem Speichern meldet es das Ergebnis ans Hauptfenster (Toast mit Rückgängig) und schließt sich.

import { useEffect } from "react";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import QuickEntry, { type Added } from "./QuickEntry";

export default function QuickWindow() {
  // kurz warten, damit die Meldung ans Hauptfenster noch rausgeht
  const close = () => window.setTimeout(() => void getCurrentWindow().close(), 150);

  // Fenster verliert den Fokus (z. B. Klick daneben): schließen, wie bei einem Spotlight-Fenster.
  // Erst nachdem es einmal den Fokus hatte, sonst schließt es sich beim Öffnen gleich wieder.
  useEffect(() => {
    let un: (() => void) | undefined;
    let hadFocus = document.hasFocus();
    void getCurrentWindow()
      .onFocusChanged(({ payload }) => {
        if (payload) hadFocus = true;
        else if (hadFocus) close();
      })
      .then((u) => (un = u));
    return () => un?.();
  }, []);

  const onAdded = (a: Added) => void emitTo("main", "quick-added", a);
  return <QuickEntry demo={false} readOnly={false} onClose={close} onAdded={onAdded} />;
}
