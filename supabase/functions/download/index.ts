// Hands the website a short-lived signed link to the latest installer in the
// private "releases" Storage bucket. CI (scripts/upload-to-storage.sh) uploads
// every tagged release to stable/latest/<name>, so this always serves the
// newest build and the website never needs a redeploy.
//
// GET /functions/v1/download?platform=mac
//   200 { version, publishedAt, size, file, url, expiresIn }
//   404 { error } when no build has been uploaded yet
//
// Sign-in on the website is a sample for now, so this doesn't check who is
// asking. When real Supabase Auth lands, verify the caller's JWT and church
// membership here before signing.

import { createClient } from "npm:@supabase/supabase-js@2";

const BUCKET = "releases";
const CHANNEL = "stable";
const EXPIRES_IN = 300; // seconds

const FILES: Record<string, string> = {
  mac: "SanctuaryMix-mac-universal.dmg",
  windows: "SanctuaryMix-windows-x64-setup.exe",
};

const cors = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, apikey, content-type",
  "Access-Control-Allow-Methods": "GET, OPTIONS",
};

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { ...cors, "Content-Type": "application/json", "Cache-Control": "no-store" },
  });

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") return new Response(null, { headers: cors });
  if (req.method !== "GET") return json(405, { error: "Use GET." });

  const platform = new URL(req.url).searchParams.get("platform") ?? "mac";
  const file = FILES[platform];
  if (!file) return json(400, { error: `Unknown platform "${platform}".` });

  const supabase = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!);
  const dir = `${CHANNEL}/latest`;
  const storage = supabase.storage.from(BUCKET);

  const { data: listing, error: listError } = await storage.list(dir, { search: file });
  const object = listing?.find((o) => o.name === file);
  if (listError || !object) return json(404, { error: "No build has been published yet." });

  // Optional: CI may also write latest.json with the version number.
  let version: string | null = null;
  let publishedAt: string | null = object.updated_at ?? object.created_at ?? null;
  const manifest = await storage.download(`${dir}/latest.json`);
  if (manifest.data) {
    try {
      const m = JSON.parse(await manifest.data.text());
      version = typeof m.version === "string" ? m.version.replace(/^v/, "") : null;
      if (typeof m.publishedAt === "string") publishedAt = m.publishedAt;
    } catch {
      // Ignore a malformed manifest; the download still works.
    }
  }

  const { data: signed, error: signError } = await storage.createSignedUrl(`${dir}/${file}`, EXPIRES_IN, {
    download: file,
  });
  if (signError || !signed) return json(500, { error: "Couldn't create a download link. Try again." });

  return json(200, {
    version,
    publishedAt,
    size: (object.metadata as { size?: number } | null)?.size ?? null,
    file,
    url: signed.signedUrl,
    expiresIn: EXPIRES_IN,
  });
});
