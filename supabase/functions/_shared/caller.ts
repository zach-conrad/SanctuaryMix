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

/** The JWT's claims. Only read after the gateway has verified the signature (verify_jwt on). */
function claims(token: string): { sub?: string; role?: string } | null {
  try {
    const part = token.split(".")[1] ?? "";
    return JSON.parse(atob(part.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(part.length / 4) * 4, "=")));
  } catch {
    return null;
  }
}

/**
 * The signed-in caller, or null when the token is missing. The gateway has
 * already verified the token (these functions deploy with verify_jwt on), so
 * the user id comes from its claims; every database call below still runs
 * with the token itself, so PostgREST checks it again. The caller client uses
 * the API key the browser sent (the project's publishable key), which works
 * with the project's current signing keys.
 */
export function getCaller(req: Request): Caller | null {
  const authorization = req.headers.get("authorization") ?? "";
  const token = authorization.match(/^Bearer\s+(\S+)$/i)?.[1];
  if (!token) return null;
  const payload = claims(token);
  if (!payload?.sub || payload.role !== "authenticated") return null;
  const apikey = req.headers.get("apikey") || Deno.env.get("SUPABASE_ANON_KEY")!;
  const asCaller = createClient(Deno.env.get("SUPABASE_URL")!, apikey, {
    global: { headers: { Authorization: `Bearer ${token}` } },
    auth: { persistSession: false, autoRefreshToken: false },
  });
  return { id: payload.sub, asCaller };
}

export function serviceClient(): SupabaseClient {
  return createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
}

export const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
