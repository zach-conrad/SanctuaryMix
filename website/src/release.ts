// Where the Download button points. The CI/CD release workflow publishes each
// tagged release as a GitHub Release on this repo and uploads the installers
// under these stable, unversioned asset names.
// GitHub's /releases/latest/download/<name> URL always redirects to the newest published (non-prerelease) release, so
// the site never needs a rebuild when a new build ships.
//
// Downloads are private: the repo is private, so the link only works for
// people signed in to GitHub with access to it. When real accounts exist,
// this should hand out a short-lived signed link instead. Change only this file.

export const RELEASE_REPO = "zach-conrad/SanctuaryMix";

// The GitHub API can't see a private repo's releases without a token, so the
// version lookup below only runs when the releases are public.
export const RELEASES_ARE_PUBLIC = false;

export const ASSETS = {
  mac: "SanctuaryMix-mac-universal.dmg",
  windows: "SanctuaryMix-windows-x64-setup.exe",
} as const;

export const downloadUrl = (asset: string) =>
  `https://github.com/${RELEASE_REPO}/releases/latest/download/${asset}`;

export const releasesPageUrl = `https://github.com/${RELEASE_REPO}/releases`;

export interface LatestRelease {
  version: string;
  publishedAt: Date;
  sizeBytes?: number;
}

export type ReleaseLookup =
  | { status: "ok"; release: LatestRelease }
  // The releases repo or its first release doesn't exist yet.
  | { status: "none" }
  // Couldn't reach the API (offline, rate limited). The button still works
  // through the stable redirect URL above.
  | { status: "unknown" };

// Best-effort lookup of the version, date and size to show next to the button.
export async function fetchLatestRelease(signal?: AbortSignal): Promise<ReleaseLookup> {
  if (!RELEASES_ARE_PUBLIC) return { status: "unknown" };
  try {
    const res = await fetch(`https://api.github.com/repos/${RELEASE_REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
      signal,
    });
    if (res.status === 404) return { status: "none" };
    if (!res.ok) return { status: "unknown" };
    const data = (await res.json()) as {
      tag_name: string;
      published_at: string;
      assets?: { name: string; size: number }[];
    };
    const mac = data.assets?.find((a) => a.name === ASSETS.mac);
    return {
      status: "ok",
      release: {
        version: data.tag_name.replace(/^v/, ""),
        publishedAt: new Date(data.published_at),
        sizeBytes: mac?.size,
      },
    };
  } catch {
    return { status: "unknown" };
  }
}
