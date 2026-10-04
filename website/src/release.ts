// Where the Download button points. Downloads are served by the website
// itself, not GitHub, so visitors never need a GitHub account.
//
// Contract with the CI/CD release workflow: on each tagged release it copies
// the installers into the built site under downloads/ and writes
// downloads/latest.json, then deploys the site:
//
//   downloads/SanctuaryMix-mac-universal.dmg
//   downloads/latest.json
//     {
//       "version": "0.2.0",
//       "publishedAt": "2026-10-04T19:00:00Z",
//       "mac": { "file": "SanctuaryMix-mac-universal.dmg", "size": 48213504 }
//     }
//
// A "windows" entry with the same shape is added once a Windows build ships.
// Until the first release is deployed there is no latest.json, and the page
// shows the button as coming soon.

export const DOWNLOADS_DIR = "downloads/";
export const MANIFEST_FILE = "latest.json";

export interface DownloadFile {
  file: string;
  size?: number;
}

export interface LatestRelease {
  version: string;
  publishedAt?: Date;
  mac: DownloadFile;
}

export type ReleaseLookup =
  | { status: "loading" }
  | { status: "ok"; release: LatestRelease }
  // No release has been deployed to the site yet.
  | { status: "none" };

/** URL of a file in downloads/, given the relative path to the site root. */
export const downloadUrl = (root: string, file: string) => `${root}${DOWNLOADS_DIR}${encodeURIComponent(file)}`;

export async function fetchLatestRelease(root: string, signal?: AbortSignal): Promise<ReleaseLookup> {
  try {
    const res = await fetch(`${root}${DOWNLOADS_DIR}${MANIFEST_FILE}`, { cache: "no-store", signal });
    if (!res.ok) return { status: "none" };
    const data = (await res.json()) as {
      version?: string;
      publishedAt?: string;
      mac?: DownloadFile;
    };
    if (!data.version || !data.mac?.file) return { status: "none" };
    return {
      status: "ok",
      release: {
        version: data.version.replace(/^v/, ""),
        publishedAt: data.publishedAt ? new Date(data.publishedAt) : undefined,
        mac: data.mac,
      },
    };
  } catch {
    return { status: "none" };
  }
}
