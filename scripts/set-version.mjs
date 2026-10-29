#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

if (!version || !SEMVER.test(version)) {
  console.error("Usage: node scripts/set-version.mjs <semver>");
  console.error("Example: node scripts/set-version.mjs 0.2.0");
  process.exit(1);
}

function updateJson(relativePath, key) {
  const path = join(root, relativePath);
  const data = JSON.parse(readFileSync(path, "utf8"));
  if (data[key] === version) return false;
  data[key] = version;
  writeFileSync(path, `${JSON.stringify(data, null, 2)}\n`);
  return true;
}

const updated = [];
if (updateJson("package.json", "version")) updated.push("package.json");
if (updateJson("src-tauri/tauri.conf.json", "version")) updated.push("src-tauri/tauri.conf.json");

const cargoPath = join(root, "src-tauri/Cargo.toml");
const lines = readFileSync(cargoPath, "utf8").split("\n");
let inPackage = false;
let cargoUpdated = false;
for (let i = 0; i < lines.length; i += 1) {
  const trimmed = lines[i].trim();
  if (trimmed === "[package]") {
    inPackage = true;
    continue;
  }
  if (!inPackage) continue;
  if (trimmed.startsWith("[") && trimmed.endsWith("]")) break;
  if (trimmed.startsWith("version")) {
    const next = lines[i].replace(/version\s*=\s*"[^"]*"/, `version = "${version}"`);
    if (next !== lines[i]) {
      lines[i] = next;
      writeFileSync(cargoPath, lines.join("\n"));
      cargoUpdated = true;
    }
    break;
  }
}
if (cargoUpdated) updated.push("src-tauri/Cargo.toml");

if (updated.length === 0) {
  console.log(`All files already at ${version}.`);
} else {
  console.log(`Set version ${version} in ${updated.join(", ")}.`);
}
console.log("Cargo.lock refreshes on the next cargo build.");
