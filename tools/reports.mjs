// Bug reports sent from the app.
//
//   node tools/reports.mjs                 list
//   node tools/reports.mjs get <id>        download to reports/<id>.zip and unzip it
//   node tools/reports.mjs get latest      newest one
//   node tools/reports.mjs delete <id>
//
// Needs .keys/admin-token (the Worker's ADMIN_TOKEN secret) and MODSYNC_BASE_URL (see .env.example).
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { unzipSync } from "fflate";

const root = resolve(import.meta.dirname, "..");
const base = (process.env.MODSYNC_BASE_URL ?? "").replace(/\/+$/, "");
if (!base) throw new Error("Set MODSYNC_BASE_URL to the Worker URL first (see .env.example)");
const token = readFileSync(join(root, ".keys", "admin-token"), "utf8").trim();
const headers = { authorization: `Bearer ${token}` };

async function list() {
  const all = [];
  let cursor = null;
  do {
    const res = await fetch(`${base}/admin/reports${cursor ? `?cursor=${cursor}` : ""}`, { headers });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const page = await res.json();
    all.push(...page.reports);
    cursor = page.cursor;
  } while (cursor);
  return all.sort((a, b) => b.id.localeCompare(a.id));
}

const [cmd, arg] = process.argv.slice(2);
if (!cmd) {
  const reports = await list();
  if (!reports.length) console.log("No reports.");
  for (const r of reports) console.log(`${r.id}  app ${r.app}  ${Math.round(r.size / 1024)} KB  ${r.note || ""}`);
} else if (cmd === "get") {
  const id = arg === "latest" ? (await list())[0]?.id : arg;
  if (!id) throw new Error("No report");
  const res = await fetch(`${base}/admin/reports/${id}`, { headers });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  const dir = join(root, "reports", id);
  mkdirSync(dir, { recursive: true });
  writeFileSync(`${dir}.zip`, bytes);
  for (const [name, data] of Object.entries(unzipSync(bytes))) {
    const p = join(dir, name);
    mkdirSync(join(p, ".."), { recursive: true });
    writeFileSync(p, data);
  }
  console.log(dir);
} else if (cmd === "delete") {
  const res = await fetch(`${base}/admin/reports/${arg}`, { method: "DELETE", headers });
  console.log(res.ok ? "deleted" : `HTTP ${res.status}`);
}
