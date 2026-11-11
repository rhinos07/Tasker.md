#!/usr/bin/env node
// Prüft vor dem Commit, dass keine privaten Daten ins öffentliche Repo gelangen (SPEC 9).
// 1. Generische Muster (Benutzerpfade, konkrete Atlassian-Domains, E-Mail-Adressen)
// 2. Persönliche Begriffe aus local/private-terms.txt (eine Zeile pro Begriff, nicht versioniert)
import { execSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

const all = process.argv.includes("--all");
const files = execSync(all ? "git ls-files -co --exclude-standard" : "git diff --cached --name-only --diff-filter=ACMR", { encoding: "utf8" })
  .split("\n")
  .filter(Boolean)
  .filter((f) => !f.startsWith("local/") && existsSync(f) && !/\.(png|ico|icns|lock)$/.test(f) && f !== "package-lock.json");

const generic = [
  { name: "Windows-Benutzerpfad", re: /[A-Za-z]:[\\/]+Users[\\/]+(?!<)[^\\/\s"'`<>]+/i },
  { name: "macOS-Benutzerpfad", re: /\/Users\/(?!<)[A-Za-z0-9._-]+\// },
  { name: "Atlassian-Domain", re: /\b(?!(?:id|api|developer|support|community|marketplace|mcp)\.)[a-z0-9-]+\.atlassian\.net\b/i },
  { name: "E-Mail-Adresse", re: /\b[A-Za-z0-9._%+-]+@(?!example\.(?:com|org)\b|anthropic\.com\b|users\.noreply\.github\.com\b)[A-Za-z][A-Za-z0-9-]*(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}\b/ },
];

const termsFile = "local/private-terms.txt";
const terms = existsSync(termsFile)
  ? readFileSync(termsFile, "utf8").split(/\r?\n/).map((s) => s.trim()).filter((s) => s && !s.startsWith("#"))
  : [];

const hits = [];
for (const f of files) {
  const text = readFileSync(f, "utf8");
  text.split(/\r?\n/).forEach((line, i) => {
    for (const g of generic) if (g.re.test(line)) hits.push(`${f}:${i + 1}  ${g.name}`);
    for (const t of terms) if (line.toLowerCase().includes(t.toLowerCase())) hits.push(`${f}:${i + 1}  privater Begriff (local/private-terms.txt)`);
  });
}

if (hits.length) {
  console.error("Private Daten gefunden, Commit abgebrochen:\n" + hits.map((h) => "  " + h).join("\n"));
  process.exit(1);
}
console.log(`Datenschutz-Check ok (${files.length} Dateien, ${terms.length} private Begriffe).`);
