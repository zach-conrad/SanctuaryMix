// Shared by the owner-admin and team-invite Edge Functions. Both are deployed
// with JWT verification on, so Supabase has already checked the caller's
// token before this runs. These helpers ask the database about the caller
// with that same token (so row-level security and the owner checks apply),
// and only then hand out a service-role client for the auth admin API.

import { createClient, type SupabaseClient } from "npm:@supabase/supabase-js@2";

/** Where email links send people. Must match an allowed redirect URL in Supabase. */
export const SITE_URL = (Deno.env.get("SITE_URL") ?? "https://sanctuarymix.vercel.app").replace(/\/+$/, "");
export const ACCOUNT_URL = `${SITE_URL}/account/`;

// The website calls these from the browser, so answer CORS preflights for it.
const ALLOWED_ORIGINS = new Set([SITE_URL, "http://localhost:5173", "http://localhost:4173"]);

export function corsHeaders(req: Request): Record<string, string> {
  const origin = req.headers.get("origin") ?? "";
  return {
    "access-control-allow-origin": ALLOWED_ORIGINS.has(origin) ? origin : SITE_URL,
    "access-control-allow-headers": "authorization, apikey, content-type, x-client-info",
    "access-control-allow-methods": "POST, OPTIONS",
    vary: "origin",
  };
}

export function reply(req: Request, status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...corsHeaders(req) },
  });
}

export interface Caller {
  id: string;
  /** Client acting as the caller: RPCs run with their permissions. */
  asCaller: SupabaseClient;
}

/** The signed-in caller, or null when the token is missing or invalid. */
export async function getCaller(req: Request): Promise<Caller | null> {
  const authorization = req.headers.get("authorization") ?? "";
  if (!/^Bearer\s+\S+$/i.test(authorization)) return null;
  const asCaller = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_ANON_KEY")!, {
    global: { headers: { authorization } },
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const { data, error } = await asCaller.auth.getUser(authorization.replace(/^Bearer\s+/i, ""));
  if (error || !data.user) return null;
  return { id: data.user.id, asCaller };
}

export function serviceClient(): SupabaseClient {
  return createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
}

export const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
