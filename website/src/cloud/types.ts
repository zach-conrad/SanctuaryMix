// The website's view of the SanctuaryMix cloud: recorded services a church has
// uploaded, and share links for them. Shapes follow the recording bundle and
// tables in docs/RECORDINGS.md and docs/supabase/, so the demo store here and
// the Supabase client later return the same data.

import type { RecordedEvent, RecordingSummary } from "../../../src/lib/types";

export type { RecordedEvent };

export type Role = "admin" | "engineer" | "volunteer";

/** Who is signed in, and the church they're working in. */
export interface CloudSession {
  userId: string;
  name: string;
  orgId: string;
  orgName: string;
  role: Role;
}

/**
 * A file stored with a recording. `listen` is the compressed copy (MP3, which every browser plays)
 * made on upload so browsers and phones can stream it; the full-quality
 * `mix.wav` and tracks stay in storage for download and the desktop app.
 */
export interface CloudFile {
  kind: "mix" | "track" | "events" | "manifest" | "listen";
  channel: number | null;
  relPath: string;
  bytes: number;
}

/** A row of `recordings` with its files. */
export interface CloudRecording extends Omit<RecordingSummary, "syncState"> {
  /** Storage prefix, `org/<org_id>/recordings/<id>`. */
  remoteKey: string;
  createdAt: number;
  updatedAt: number;
  /** Soft delete, so deletions sync; the list hides these. */
  deletedAt: number | null;
  files: CloudFile[];
}

export interface ShareOptions {
  /** Days until the link stops working; null keeps it on until turned off. */
  expiresInDays: number | null;
  /** Include the fader and mute timeline, not just the audio. */
  showMoves: boolean;
  /** Include the EQ audit (soundcheck EQ, feedback cuts, tone moves). Off unless chosen. */
  showEq: boolean;
}

/** What AI EQ or a person did to one channel's EQ. */
export type EqAction =
  | "soundcheck" // AI EQ proposal applied at soundcheck
  | "feedback" // AI EQ caught ringing and cut it
  | "tone" // AI EQ kept a speech mic's tone
  | "undo" // someone undid an AI EQ change
  | "person" // someone changed EQ by hand (desk or app); the channel's EQ is theirs
  | "handBack"; // someone gave the channel back to AI EQ

/** Who made an EQ change. `name` is only filled for the church's own members. */
export interface EqActor {
  kind: "ai" | "person";
  role: Role | null;
  name: string | null;
  /** Where a person made it: in the app or on the desk itself. */
  where?: "app" | "desk";
}

export interface EqAuditEntry {
  seq: number;
  /** Position in the recording, or null for soundcheck before recording began. */
  tMs: number | null;
  /** Wall-clock time, epoch ms. */
  at: number;
  channel: { name: string; label: string };
  action: EqAction;
  /** Exact changes, e.g. "Low cut  off → 100 Hz", "320 Hz  0.0 → −3.0 dB". */
  changes: string[];
  /** One sentence a volunteer can check by ear (AI changes only). */
  reason: string | null;
  /** For AI changes applied by a person (soundcheck), who tapped Apply. */
  appliedBy: EqActor | null;
  by: EqActor;
}

/** An idea AI EQ collected for next Sunday (account only, never shared). */
export interface EqIdea {
  channel: { name: string; label: string };
  title: string;
  change: string;
  reason: string;
  state: "waiting" | "kept" | "dismissed";
}

export interface EqAudit {
  entries: EqAuditEntry[];
  /** Empty on share links. */
  ideas: EqIdea[];
}

/** A row of `share_links` (docs/supabase/share_links.sql). */
export interface ShareLink {
  id: string;
  recordingId: string;
  /** Unguessable secret that goes in the link. */
  token: string;
  createdBy: string;
  createdAt: number;
  expiresAt: number | null;
  revokedAt: number | null;
  showMoves: boolean;
  showEq: boolean;
}

/** What a share link opens, for anyone who has it (no sign-in). */
export type SharedMix =
  | {
      status: "ok";
      orgName: string;
      recording: CloudRecording;
      /** Empty when the link was made without moves. */
      events: RecordedEvent[];
      audioUrl: string | null;
      showMoves: boolean;
      /** Null when the link was made without EQ. People's names are never included. */
      eq: EqAudit | null;
      expiresAt: number | null;
      /** Set by the demo store when it stands in for a link it has no record of. */
      previewNote?: string;
    }
  | { status: "expired" | "revoked" | "notFound" };

/**
 * The cloud boundary for the website, like `AuthProvider` and `RecordingSync`
 * in the app. `AccountCloud` serves the signed-in church (no recordings until
 * they sync from the app); `DemoCloud` serves the share page's sample from static files;
 * a Supabase implementation replaces it without touching the pages.
 */
export interface RecordingsCloud {
  session(): CloudSession;
  /** The signed-in church's services, newest first. */
  listRecordings(): Promise<CloudRecording[]>;
  getRecording(id: string): Promise<CloudRecording | null>;
  /** Every control change in recording order (snapshot first). */
  getEvents(recording: CloudRecording): Promise<RecordedEvent[]>;
  /** A short-lived URL for the listening copy, or null when there's no audio. */
  listenUrl(recording: CloudRecording): Promise<string | null>;
  /** Every EQ change around the service, with names: members always see all of it. */
  getEqAudit(recording: CloudRecording): Promise<EqAudit>;

  listShareLinks(recordingId: string): Promise<ShareLink[]>;
  createShareLink(recordingId: string, options: ShareOptions): Promise<ShareLink>;
  revokeShareLink(id: string): Promise<void>;
  /** Public: resolve a link's token without signing in. */
  openShare(token: string): Promise<SharedMix>;
}

export const storageKey = (orgId: string, recordingId: string, relPath = ""): string =>
  `org/${orgId}/recordings/${recordingId}${relPath ? `/${relPath}` : ""}`;

/**
 * What a share link may show of an EQ audit: roles but no names, and none of
 * the ideas for next week. `open_share` in docs/supabase/share_links.sql
 * applies the same rule in the database.
 */
export function eqForShareLink(audit: EqAudit): EqAudit {
  const anon = (a: EqActor): EqActor => ({ ...a, name: null });
  return {
    entries: audit.entries.map((e) => ({ ...e, by: anon(e.by), appliedBy: e.appliedBy && anon(e.appliedBy) })),
    ideas: [],
  };
}

export const canShare = (role: Role): boolean => role === "admin" || role === "engineer";

export function shareUrl(root: string, token: string): string {
  return new URL(`${root}share/?t=${encodeURIComponent(token)}`, window.location.href).toString();
}

export function isLive(link: ShareLink, now = Date.now()): boolean {
  return link.revokedAt === null && (link.expiresAt === null || link.expiresAt > now);
}
