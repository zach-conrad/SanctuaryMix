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
      expiresAt: number | null;
      /** Set by the demo store when it stands in for a link it has no record of. */
      previewNote?: string;
    }
  | { status: "expired" | "revoked" | "notFound" };

/**
 * The cloud boundary for the website, like `AuthProvider` and `RecordingSync`
 * in the app. Today `DemoCloud` serves the sample account from static files;
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

  listShareLinks(recordingId: string): Promise<ShareLink[]>;
  createShareLink(recordingId: string, options: ShareOptions): Promise<ShareLink>;
  revokeShareLink(id: string): Promise<void>;
  /** Public: resolve a link's token without signing in. */
  openShare(token: string): Promise<SharedMix>;
}

export const storageKey = (orgId: string, recordingId: string, relPath = ""): string =>
  `org/${orgId}/recordings/${recordingId}${relPath ? `/${relPath}` : ""}`;

export const canShare = (role: Role): boolean => role === "admin" || role === "engineer";

export function shareUrl(root: string, token: string): string {
  return new URL(`${root}share/?t=${encodeURIComponent(token)}`, window.location.href).toString();
}

export function isLive(link: ShareLink, now = Date.now()): boolean {
  return link.revokedAt === null && (link.expiresAt === null || link.expiresAt > now);
}
