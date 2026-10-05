// Sample stand-in for the public share page until recordings sync from the
// app: serves a sample church's service from static files laid out exactly
// like Supabase Storage keys (public/demo-cloud/org/<org_id>/recordings/<id>/...).
// Accounts are real (see src/auth); nothing here signs anyone in.

import {
  isLive,
  storageKey,
  type CloudRecording,
  type CloudSession,
  type RecordedEvent,
  type RecordingsCloud,
  type ShareLink,
  type ShareOptions,
  type SharedMix,
} from "./types";

const DAY_MS = 86_400_000;
const LINKS_KEY = "sanctuarymix.demo.shareLinks";

export const SAMPLE_SESSION: CloudSession = {
  userId: "0199a0c2-7b40-7e21-8f3a-1d5c9e6b2a08",
  name: "Sample engineer",
  orgId: "0199a0c2-5e1d-7a3c-9b41-6c2f0e8d4a17",
  orgName: "Grace Community Church",
  role: "engineer",
};

/** 128 random bits, base64url: the secret part of a share link. */
export function newToken(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function newId(): string {
  return typeof crypto.randomUUID === "function" ? crypto.randomUUID() : newToken();
}

export function parseEvents(jsonl: string): RecordedEvent[] {
  return jsonl
    .split("\n")
    .filter((line) => line.trim())
    .map((line) => JSON.parse(line) as RecordedEvent)
    .sort((a, b) => a.seq - b.seq);
}

function readLinks(): ShareLink[] {
  try {
    const raw = localStorage.getItem(LINKS_KEY);
    return raw ? (JSON.parse(raw) as ShareLink[]) : [];
  } catch {
    return [];
  }
}

function writeLinks(links: ShareLink[]): void {
  try {
    localStorage.setItem(LINKS_KEY, JSON.stringify(links));
  } catch {
    // Private windows can refuse storage; the link still works for this visit.
  }
}

export class DemoCloud implements RecordingsCloud {
  private recordings: Promise<CloudRecording[]> | null = null;
  private memoryLinks: ShareLink[] = readLinks();

  /** `root` is the relative path from the current page to the site root. */
  constructor(private readonly root: string) {}

  session(): CloudSession {
    return SAMPLE_SESSION;
  }

  private url(key: string): string {
    return `${this.root}demo-cloud/${key}`;
  }

  listRecordings(): Promise<CloudRecording[]> {
    this.recordings ??= fetch(this.url(`org/${SAMPLE_SESSION.orgId}/recordings.json`))
      .then((r) => {
        if (!r.ok) throw new Error(`Couldn't load services (${r.status})`);
        return r.json() as Promise<{ recordings: CloudRecording[] }>;
      })
      .then((body) => body.recordings.filter((r) => r.deletedAt === null).sort((a, b) => b.startedAt - a.startedAt))
      .catch((e) => {
        this.recordings = null;
        throw e;
      });
    return this.recordings;
  }

  async getRecording(id: string): Promise<CloudRecording | null> {
    return (await this.listRecordings()).find((r) => r.id === id) ?? null;
  }

  async getEvents(recording: CloudRecording): Promise<RecordedEvent[]> {
    const r = await fetch(this.url(storageKey(recording.orgId!, recording.id, "events.jsonl")));
    if (!r.ok) throw new Error(`Couldn't load the moves (${r.status})`);
    return parseEvents(await r.text());
  }

  async listenUrl(recording: CloudRecording): Promise<string | null> {
    const file = recording.files.find((f) => f.kind === "listen");
    return file ? this.url(storageKey(recording.orgId!, recording.id, file.relPath)) : null;
  }

  async listShareLinks(recordingId: string): Promise<ShareLink[]> {
    return this.memoryLinks.filter((l) => l.recordingId === recordingId).sort((a, b) => b.createdAt - a.createdAt);
  }

  async createShareLink(recordingId: string, options: ShareOptions): Promise<ShareLink> {
    const now = Date.now();
    const link: ShareLink = {
      id: newId(),
      recordingId,
      token: newToken(),
      createdBy: SAMPLE_SESSION.userId,
      createdAt: now,
      expiresAt: options.expiresInDays === null ? null : now + options.expiresInDays * DAY_MS,
      revokedAt: null,
      showMoves: options.showMoves,
    };
    this.memoryLinks = [...this.memoryLinks, link];
    writeLinks(this.memoryLinks);
    return link;
  }

  async revokeShareLink(id: string): Promise<void> {
    this.memoryLinks = this.memoryLinks.map((l) => (l.id === id && l.revokedAt === null ? { ...l, revokedAt: Date.now() } : l));
    writeLinks(this.memoryLinks);
  }

  async openShare(token: string): Promise<SharedMix> {
    if (!/^[A-Za-z0-9_-]{16,64}$/.test(token)) return { status: "notFound" };
    // Re-read so a link turned off in another tab is honored.
    this.memoryLinks = readLinks().length ? readLinks() : this.memoryLinks;
    const link = this.memoryLinks.find((l) => l.token === token);
    if (link && link.revokedAt !== null) return { status: "revoked" };
    if (link && !isLive(link)) return { status: "expired" };

    const list = await this.listRecordings();
    const recording = (link && list.find((r) => r.id === link.recordingId)) ?? list[0];
    if (!recording) return { status: "notFound" };
    const showMoves = link?.showMoves ?? true;
    return {
      status: "ok",
      orgName: SAMPLE_SESSION.orgName,
      recording,
      events: showMoves ? await this.getEvents(recording) : [],
      audioUrl: await this.listenUrl(recording),
      showMoves,
      expiresAt: link?.expiresAt ?? null,
      // A link made in another browser isn't in this one's storage. Until the
      // cloud is connected, play the sample service rather than fail.
      previewNote: link
        ? undefined
        : "Share links aren't connected to accounts yet, so this preview plays the sample service.",
    };
  }
}
