import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import QuickWindow from "./QuickWindow";
import "./styles.css";

// Das kleine Fenster der Schnellerfassung (globales Tastenkürzel) lädt dieselbe Seite mit #quick.
const quick = window.location.hash === "#quick";
if (quick) document.documentElement.classList.add("quick-window");

createRoot(document.getElementById("root")!).render(<StrictMode>{quick ? <QuickWindow /> : <App />}</StrictMode>);
