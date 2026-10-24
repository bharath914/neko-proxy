// Finds the newest desktop release (tags v*) and its downloads, once per page load.
// Falls back to null if GitHub can't be reached or nothing is published yet.
const API = 'https://api.github.com/repos/bharath914/neko-proxy/releases?per_page=20';

let pending;

export function latestRelease() {
	pending ??= fetch(API, { headers: { Accept: 'application/vnd.github+json' } })
		.then((r) => (r.ok ? r.json() : []))
		.then(pick)
		.catch(() => null);
	return pending;
}

function pick(list) {
	const desktop = list.filter((r) => !r.draft && /^v\d/.test(r.tag_name));
	const rel = desktop.find((r) => !r.prerelease) ?? desktop[0];
	if (!rel) return null;
	const find = (re) => {
		const a = rel.assets.find((x) => re.test(x.name));
		return a ? { name: a.name, url: a.browser_download_url, size: a.size } : null;
	};
	return {
		version: rel.tag_name,
		url: rel.html_url,
		prerelease: rel.prerelease,
		date: rel.published_at,
		mac: find(/-macos-universal\.dmg$/) ?? find(/-macos-arm64\.dmg$/) ?? find(/\.dmg$/),
		appimage: { x86_64: find(/-x86_64\.AppImage$/), aarch64: find(/-aarch64\.AppImage$/) },
		deb: { x86_64: find(/_amd64\.deb$/), aarch64: find(/_arm64\.deb$/) },
		rpm: { x86_64: find(/\.x86_64\.rpm$/), aarch64: find(/\.aarch64\.rpm$/) },
		tar: { x86_64: find(/-linux-x86_64\.tar\.gz$/), aarch64: find(/-linux-aarch64\.tar\.gz$/) },
		sums: find(/^SHA256SUMS$/)
	};
}

/** The visitor's OS and CPU, as well as the browser will say. */
export async function detectPlatform() {
	const ua = navigator.userAgent || '';
	const os = /iPhone|iPad|iPod|Android/.test(ua) ? 'mobile'
		: /Mac OS X|Macintosh/.test(ua) ? (navigator.maxTouchPoints > 1 ? 'mobile' : 'mac')
		: /Windows/.test(ua) ? 'windows'
		: /Linux|X11|CrOS/.test(ua) ? 'linux' : 'other';
	let arch = /aarch64|arm64/i.test(ua) ? 'aarch64' : 'x86_64';
	try {
		const hints = await navigator.userAgentData?.getHighEntropyValues?.(['architecture']);
		if (hints?.architecture === 'arm') arch = 'aarch64';
	} catch {}
	return { os, arch };
}

export const mb = (bytes) => `${(bytes / 1048576).toFixed(bytes > 10485760 ? 0 : 1)} MB`;
