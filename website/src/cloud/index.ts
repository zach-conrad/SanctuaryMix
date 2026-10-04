import { DemoCloud } from "./demo";
import type { RecordingsCloud } from "./types";

export * from "./types";

/**
 * The cloud the pages use. `root` is the relative path from the page to the
 * site root. Swap DemoCloud for the Supabase client here when accounts land.
 */
export function connectCloud(root: string): RecordingsCloud {
  return new DemoCloud(root);
}
