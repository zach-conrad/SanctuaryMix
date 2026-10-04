// Where the Download button points. The CI/CD pipeline publishes each release
// as a GitHub Release on this repo and uploads the installers under these
// stable, unversioned asset names. GitHub's /releases/latest/download/<name>
// URL always redirects to the newest published (non-prerelease) release, so
// the site never needs a rebuild when a new build ships.
//
// If downloads move (a public releases repo, a CDN, the site host), change
// only this file.

export const RELEASE_REPO = "zach-conrad/SanctuaryMix";

export const ASSETS = {
  mac: "SanctuaryMix-macOS-universal.dmg",
} as const;

export const downloadUrl = (asset: string) =>
  `https://github.com/${RELEASE_REPO}/releases/latest/download/${asset}`;

export const releasesPageUrl = `https://github.com/${RELEASE_REPO}/releases`;

export interface LatestRelease {
  version: string;
  publishedAt: Date;
  sizeBytes?: number;
}

// Best-effort lookup of the version and date to show next to the button.
// Returns null when the API can't be reached or the repo is private; the
// button still works through the stable redirect URL above.
export async function fetchLatestRelease(signal?: AbortSignal): Promise<LatestRelease | null> {
  try {
    const res = await fetch(`https://api.github.com/repos/${RELEASE_REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
      signal,
    });
    if (!res.ok) return null;
    const data = (await res.json()) as {
      tag_name: string;
      published_at: string;
      assets?: { name: string; size: number }[];
    };
    const mac = data.assets?.find((a) => a.name === ASSETS.mac);
    return {
      version: data.tag_name.replace(/^v/, ""),
      publishedAt: new Date(data.published_at),
      sizeBytes: mac?.size,
    };
  } catch {
    return null;
  }
}
