// Mod Sync backend. Static files (catalog, packages, app updates, landing page) are
// served by Workers Assets before this code runs; this only handles bug reports and the admin API.

export interface Env {
  ASSETS: Fetcher;
  REPORTS: KVNamespace;
  ADMIN_TOKEN?: string;
}

const MAX_REPORT = 10 * 1024 * 1024;

const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json" } });

function authorized(req: Request, env: Env) {
  const token = req.headers.get("authorization")?.replace(/^Bearer\s+/i, "");
  return !!env.ADMIN_TOKEN && token === env.ADMIN_TOKEN;
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);

    if (url.pathname === "/reports" && req.method === "POST") {
      const size = Number(req.headers.get("content-length") ?? 0);
      if (size > MAX_REPORT) return json({ error: "too large" }, 413);
      const body = await req.arrayBuffer();
      if (body.byteLength === 0 || body.byteLength > MAX_REPORT) return json({ error: "bad size" }, 400);
      const game = (req.headers.get("x-game") ?? "app").replace(/[^a-z0-9_-]/gi, "").slice(0, 32) || "app";
      const id = `${new Date().toISOString().replace(/[:.]/g, "-")}-${game}-${crypto.randomUUID().slice(0, 8)}`;
      const metadata = {
        game,
        app: (req.headers.get("x-app-version") ?? "").slice(0, 32),
        note: decodeURIComponent(req.headers.get("x-note") ?? "").slice(0, 500),
        size: body.byteLength,
        country: (req as any).cf?.country ?? "",
      };
      // Reports expire after 90 days.
      await env.REPORTS.put(`r/${id}`, body, { metadata, expirationTtl: 60 * 60 * 24 * 90 });
      return json({ id });
    }

    if (url.pathname.startsWith("/admin/")) {
      if (!authorized(req, env)) return json({ error: "unauthorized" }, 401);
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
