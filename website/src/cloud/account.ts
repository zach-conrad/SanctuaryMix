// The signed-in church's cloud. Accounts are real (Supabase); recorded
// services aren't synced from the app yet, so the library is empty until the
// app uploads them (draft schema: docs/supabase/recordings.sql and
// share_links.sql). Pages already handle the empty state.

import type {
  CloudRecording,
  CloudSession,
  EqAudit,
  RecordedEvent,
  RecordingsCloud,
  ShareLink,
  SharedMix,
} from "./types";

export class AccountCloud implements RecordingsCloud {
  constructor(private readonly who: CloudSession) {}

  session(): CloudSession {
    return this.who;
  }

  async listRecordings(): Promise<CloudRecording[]> {
    return [];
  }

  async getRecording(): Promise<CloudRecording | null> {
    return null;
  }

  async getEvents(): Promise<RecordedEvent[]> {
    return [];
  }

  async listenUrl(): Promise<string | null> {
    return null;
  }

  async getEqAudit(): Promise<EqAudit> {
    return { entries: [], ideas: [] };
  }

  async listShareLinks(): Promise<ShareLink[]> {
    return [];
  }

  async createShareLink(): Promise<ShareLink> {
    throw new Error("Share links arrive with recording sync");
  }

  async revokeShareLink(): Promise<void> {}

  async openShare(): Promise<SharedMix> {
    return { status: "notFound" };
  }
}
