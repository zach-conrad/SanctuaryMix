// Where the Download button gets its file. Installers live in a PRIVATE
// Supabase Storage bucket ("releases"). The CI/CD release workflow uploads
// every tagged release to stable/latest/, so the newest build is always what
// visitors get, with no website redeploy. Visitors never see a permanent
// link: the `download` Edge Function (supabase/functions/download) signs a
// link that expires after a few minutes.
//
// Build-time settings (website/.env or CI):
//   VITE_SUPABASE_URL       https://<project>.supabase.co
//   VITE_SUPABASE_ANON_KEY  the project's public anon key (optional for now)
// Without VITE_SUPABASE_URL the page shows the download as coming soon.

export type Platform = "mac" | "windows";

export interface LatestRelease {
  version: string | null;
  publishedAt: Date | null;
  size: number | null;
  file: string;
  url: string;
}

export type ReleaseLookup =
  | { status: "loading" }
  | { status: "ok"; release: LatestRelease }
  // Supabase isn't set up yet, or no build has been uploaded.
  | { status: "none" }
  // The download service couldn't be reached.
  | { status: "error" };

const SUPABASE_URL = (import.meta.env.VITE_SUPABASE_URL as string | undefined)?.replace(/\/$/, "");
const ANON_KEY = import.meta.env.VITE_SUPABASE_ANON_KEY as string | undefined;

/** Asks the download function for the latest build and a fresh signed link. */
export async function getDownload(platform: Platform, signal?: AbortSignal): Promise<ReleaseLookup> {
  if (!SUPABASE_URL) return { status: "none" };
  try {
    const res = await fetch(`${SUPABASE_URL}/functions/v1/download?platform=${platform}`, {
      headers: ANON_KEY ? { Authorization: `Bearer ${ANON_KEY}`, apikey: ANON_KEY } : {},
      cache: "no-store",
      signal,
    });
    if (res.status === 404) return { status: "none" };
    if (!res.ok) return { status: "error" };
    const data = (await res.json()) as {
      version: string | null;
      publishedAt: string | null;
      size: number | null;
      file: string;
      url: string;
    };
    return {
      status: "ok",
      release: {
        version: data.version,
        publishedAt: data.publishedAt ? new Date(data.publishedAt) : null,
        size: data.size,
        file: data.file,
        url: data.url,
      },
    };
  } catch {
    return { status: "error" };
  }
}
