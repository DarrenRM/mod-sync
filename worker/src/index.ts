// Mod Sync backend. Static files (catalog, packages, app updates, landing page) are
// served by Workers Assets before this code runs; this only handles bug reports and the admin API.

export interface Env {
  ASSETS: Fetcher;
  REPORTS: KVNamespace;
  ADMIN_TOKEN?: string;
}

const MAX_REPORT = 10 * 1024 * 1024;
// KV allows 1024 bytes of metadata per key; the note gets most of it, the rest stays well under.
const MAX_NOTE_BYTES = 300;

const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json" } });

async function authorized(req: Request, env: Env) {
  if (!env.ADMIN_TOKEN) return false;
  const token = req.headers.get("authorization")?.replace(/^Bearer\s+/i, "") ?? "";
  // Compare fixed-length digests in constant time, so response timing reveals nothing about the token.
  const [a, b] = await Promise.all(
    [token, env.ADMIN_TOKEN].map((s) => crypto.subtle.digest("SHA-256", new TextEncoder().encode(s))),
  );
  return crypto.subtle.timingSafeEqual(a, b);
}

/** The longest prefix of `s` that fits in `max` UTF-8 bytes, without splitting a character. */
function clipUtf8(s: string, max: number) {
  let bytes = 0;
  let end = 0;
  for (const ch of s) {
    const cp = ch.codePointAt(0)!;
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (bytes > max) break;
    end += ch.length;
  }
  return s.slice(0, end);
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);

    if (url.pathname === "/reports" && req.method === "POST") {
      const size = Number(req.headers.get("content-length") ?? 0);
      if (size > MAX_REPORT) return json({ error: "too large" }, 413);
      // The app percent-encodes the note; a malformed one is the sender's mistake, not a server error.
      let note: string;
      try {
        note = decodeURIComponent(req.headers.get("x-note") ?? "");
      } catch {
        return json({ error: "bad x-note" }, 400);
      }
      const body = await req.arrayBuffer();
      if (body.byteLength === 0 || body.byteLength > MAX_REPORT) return json({ error: "bad size" }, 400);
      const game = (req.headers.get("x-game") ?? "app").replace(/[^a-z0-9_-]/gi, "").slice(0, 32) || "app";
      const id = `${new Date().toISOString().replace(/[:.]/g, "-")}-${game}-${crypto.randomUUID().slice(0, 8)}`;
      const metadata = {
        game,
        app: clipUtf8(req.headers.get("x-app-version") ?? "", 32),
        note: clipUtf8(note, MAX_NOTE_BYTES),
        size: body.byteLength,
        country: (req as any).cf?.country ?? "",
      };
      // Reports expire after 90 days.
      await env.REPORTS.put(`r/${id}`, body, { metadata, expirationTtl: 60 * 60 * 24 * 90 });
      return json({ id });
    }

    if (url.pathname.startsWith("/admin/")) {
      if (!(await authorized(req, env))) return json({ error: "unauthorized" }, 401);
      if (url.pathname === "/admin/reports") {
        const list = await env.REPORTS.list({ prefix: "r/", cursor: url.searchParams.get("cursor") ?? undefined });
        return json({
          reports: list.keys.map((k) => ({ id: k.name.slice(2), ...(k.metadata as object) })),
          cursor: list.list_complete ? null : list.cursor,
        });
      }
      const m = url.pathname.match(/^\/admin\/reports\/([\w-]+)$/);
      if (m && req.method === "GET") {
        const body = await env.REPORTS.get(`r/${m[1]}`, "arrayBuffer");
        if (!body) return json({ error: "not found" }, 404);
        return new Response(body, {
          headers: { "content-type": "application/zip", "content-disposition": `attachment; filename="${m[1]}.zip"` },
        });
      }
      if (m && req.method === "DELETE") {
        await env.REPORTS.delete(`r/${m[1]}`);
        return json({ ok: true });
      }
      return json({ error: "not found" }, 404);
    }

    // The catalog and the app update manifest must never be served stale after a deploy.
    if (url.pathname === "/catalog.json" || url.pathname === "/app/latest.json") {
      const res = await env.ASSETS.fetch(new Request(new URL(url.pathname, url.origin), req));
      const out = new Response(res.body, res);
      out.headers.set("cache-control", "no-store");
      out.headers.delete("etag");
      return out;
    }

    return env.ASSETS.fetch(req);
  },
} satisfies ExportedHandler<Env>;
