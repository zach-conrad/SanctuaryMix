import { AccountCloud } from "./account";
import { DemoCloud } from "./demo";
import type { CloudSession, RecordingsCloud } from "./types";

export * from "./types";

/** The signed-in church's cloud, for the account and service pages. */
export function connectCloud(session: CloudSession): RecordingsCloud {
  return new AccountCloud(session);
}

/**
 * The public share page. Until recordings sync from the app, links play the
 * bundled sample service. `root` is the relative path from the page to the
 * site root.
 */
export function connectShareCloud(root: string): RecordingsCloud {
  return new DemoCloud(root);
}
