#!/usr/bin/env node
// Versionsnummer an allen drei Stellen setzen oder prüfen: package.json, Cargo.toml (workspace),
// src-tauri/tauri.conf.json.
//
//   node scripts/version.mjs            zeigt die Versionen
//   node scripts/version.mjs set 0.2.0  setzt sie überall (und aktualisiert Cargo.lock)
//   node scripts/version.mjs check v0.2.0   bricht ab, wenn eine Stelle nicht passt (für den Release-Workflow)

import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";

const files = {
  "package.json": {
    get: (t) => JSON.parse(t).version,
    set: (t, v) => t.replace(/("version":\s*")[^"]+(")/, `$1${v}$2`),
  },
  "src-tauri/tauri.conf.json": {
    get: (t) => JSON.parse(t).version,
    set: (t, v) => t.replace(/("version":\s*")[^"]+(")/, `$1${v}$2`),
  },
  "Cargo.toml": {
    get: (t) => /\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/.exec(t)?.[1],
    set: (t, v) => t.replace(/(\[workspace\.package\][^[]*?\nversion\s*=\s*")[^"]+(")/, `$1${v}$2`),
  },
};

const read = () => Object.fromEntries(Object.entries(files).map(([f, h]) => [f, h.get(readFileSync(f, "utf8"))]));
const [cmd, arg] = process.argv.slice(2);

if (cmd === "set") {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/.test(arg ?? "")) {
    console.error("Bitte eine Version wie 0.2.0 angeben.");
    process.exit(1);
  }
  for (const [f, h] of Object.entries(files)) writeFileSync(f, h.set(readFileSync(f, "utf8"), arg));
  // package-lock.json: nur die eigene Version (oben und beim Wurzelpaket)
  const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
  lock.version = arg;
  if (lock.packages?.[""]) lock.packages[""].version = arg;
  writeFileSync("package-lock.json", JSON.stringify(lock, null, 2) + "\n");
  execSync("cargo update --workspace --offline", { stdio: "inherit" });
  console.log(`Version ${arg} gesetzt. Danach committen und Tag v${arg} pushen.`);
} else {
  const v = read();
  for (const [f, x] of Object.entries(v)) console.log(`${x ?? "?"}\t${f}`);
  const distinct = new Set(Object.values(v));
  if (distinct.size !== 1) {
    console.error("Die Versionen stimmen nicht überein. Mit `node scripts/version.mjs set X.Y.Z` angleichen.");
    process.exit(1);
  }
  if (cmd === "check" && arg && arg.replace(/^v/, "") !== [...distinct][0]) {
    console.error(`Tag ${arg} passt nicht zur Version ${[...distinct][0]}.`);
    process.exit(1);
  }
}
