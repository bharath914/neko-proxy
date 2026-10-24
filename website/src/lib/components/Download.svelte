<script>
	import { onMount } from 'svelte';
	import { asset } from '$app/paths';
	import { reveal } from '#lib/reveal.js';
	import { repo, releases } from '#lib/site.js';
	import { latestRelease, detectPlatform, mb } from '#lib/release.js';
	import Code from './Code.svelte';

	let rel = $state(null);
	let loaded = $state(false);
	let os = $state('');
	let arch = $state('x86_64');

	onMount(async () => {
		const [r, p] = await Promise.all([latestRelease(), detectPlatform()]);
		rel = r;
		os = p.os;
		arch = p.arch;
		loaded = true;
	});

	const other = (a) => (a === 'x86_64' ? 'aarch64' : 'x86_64');
	const archName = (a) => (a === 'x86_64' ? 'x86_64 (Intel/AMD)' : 'aarch64 (ARM)');
	let appimage = $derived(rel?.appimage[arch] ?? rel?.appimage.x86_64);

	const source = [
		{
			name: 'Linux',
			code: `git clone ${repo}.git
cd neko-proxy
cargo run --release -p neko-desktop
# needs only Rust from rustup.rs`
		},
		{
			name: 'macOS',
			code: `git clone ${repo}.git
cd neko-proxy
cargo build -p neko-cli
cd desktop-macos && swift run Neko`
		},
		{
			name: 'Headless',
			code: `cargo build --release -p neko-cli
target/release/nekod run          # proxy on 127.0.0.1:8899
target/release/nekod flows 'status:5xx'
target/release/nekod export --har -o session.har`
		}
	];
	let tab = $state(0);
</script>

<section class="block" id="download">
	<div class="wrap">
		<div class="center" use:reveal>
			<img class="icon" src={asset('/assets/neko.png')} alt="" width="88" height="88" />
			<h2 class="section-title">Get Neko.</h2>
			<p class="section-lede">
				{#if rel}
					Version {rel.version.replace(/^v/, '')}{rel.prerelease ? ' (preview)' : ''}. Free, open source, and yours to keep.
				{:else}
					Free, open source, and yours to keep.
				{/if}
			</p>
		</div>

		<div class="cards">
			<article class="card plat" class:mine={os === 'mac'} use:reveal>
				<svg class="logo" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M16.37 12.62c-.02-2.2 1.8-3.26 1.88-3.31-1.03-1.5-2.62-1.7-3.18-1.73-1.35-.14-2.64.8-3.33.8-.69 0-1.74-.78-2.87-.76-1.47.02-2.83.86-3.59 2.18-1.53 2.66-.39 6.59 1.1 8.75.73 1.05 1.6 2.24 2.73 2.2 1.1-.05 1.51-.71 2.84-.71 1.32 0 1.7.71 2.86.69 1.18-.02 1.93-1.07 2.65-2.13.84-1.22 1.18-2.4 1.2-2.46-.03-.01-2.3-.88-2.32-3.52zM14.19 6.15c.6-.73 1.01-1.75.9-2.76-.87.04-1.92.58-2.54 1.31-.56.64-1.05 1.67-.92 2.66.97.08 1.96-.49 2.56-1.21z"/></svg>
				<h3>macOS</h3>
				<p class="req">macOS 13 or later · Apple silicon and Intel</p>
				{#if rel?.mac}
					<a class="btn lg" href={rel.mac.url}>Download .dmg</a>
					<p class="meta">{rel.mac.name} · {mb(rel.mac.size)}</p>
					<details class="hint">
						<summary>If macOS says it can’t verify Neko</summary>
						<p>Neko isn’t notarized by Apple yet, so macOS asks once. Click <b>Done</b>, open <b>System Settings › Privacy &amp; Security</b>, scroll to <i>“Neko” was blocked</i> and click <b>Open Anyway</b>. Or run:</p>
						<code>xattr -dr com.apple.quarantine /Applications/Neko.app</code>
					</details>
				{:else}
					<a class="btn lg secondary" href={releases}>{loaded ? 'First release soon' : 'See releases'}</a>
					<p class="meta">Or build it from source below. Takes about a minute.</p>
				{/if}
			</article>

			<article class="card plat" class:mine={os === 'linux'} use:reveal={80}>
				<svg class="logo" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M12.5 2c-1.9 0-3 1.6-3 3.9 0 1.3.3 2.3-.4 3.5-.8 1.4-2.6 3.3-2.6 6.1 0 .6.1 1.2.3 1.7-.8.4-1.9.6-2.2 1.4-.3.9.6 1.4 1.7 1.7 1.2.3 2.2 1.1 3.1 1.1.8 0 1.3-.5 1.6-1.1h3.6c.3.6.9 1.1 1.7 1.1.9 0 1.9-.8 3.1-1.1 1.1-.3 2-.8 1.7-1.7-.3-.8-1.4-1-2.2-1.4.2-.5.3-1.1.3-1.7 0-2.8-1.8-4.7-2.6-6.1-.7-1.2-.4-2.2-.4-3.5C16 3.6 14.4 2 12.5 2zm-1.2 3.4c.4 0 .7.5.7 1.1s-.3 1.1-.7 1.1-.7-.5-.7-1.1.3-1.1.7-1.1zm2.6 0c.4 0 .7.5.7 1.1s-.3 1.1-.7 1.1-.7-.5-.7-1.1.3-1.1.7-1.1zm-1.3 3c.8 0 1.8.5 1.8.9s-1 .9-1.8.9-1.8-.5-1.8-.9.9-.9 1.8-.9z"/></svg>
				<h3>Linux</h3>
				<p class="req">glibc 2.35+ · Ubuntu 22.04, Debian 12, Fedora 36, RHEL 9 and newer</p>
				{#if appimage}
					<a class="btn lg" href={appimage.url}>Download AppImage</a>
					<p class="meta">{appimage.name} · {mb(appimage.size)}</p>
					<div class="more">
						{#each [arch, other(arch)] as a}
							<div class="arch">
								<span>{archName(a)}</span>
								{#if rel.appimage[a] && a !== arch}<a href={rel.appimage[a].url}>AppImage</a>{/if}
								{#if rel.deb[a]}<a href={rel.deb[a].url}>.deb</a>{/if}
								{#if rel.rpm[a]}<a href={rel.rpm[a].url}>.rpm</a>{/if}
								{#if rel.tar[a]}<a href={rel.tar[a].url}>.tar.gz</a>{/if}
							</div>
						{/each}
					</div>
				{:else}
					<a class="btn lg secondary" href={releases}>{loaded ? 'First release soon' : 'See releases'}</a>
					<p class="meta">.deb, .rpm, AppImage and a tarball, for x86_64 and aarch64.</p>
				{/if}
			</article>

			<article class="card plat soon" class:mine={os === 'windows'} use:reveal={160}>
				<svg class="logo" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M3 5.5 10.5 4.5v7H3zM11.5 4.3 21 3v8.5h-9.5zM3 12.5h7.5v7L3 18.5zM11.5 12.5H21V21l-9.5-1.3z"/></svg>
				<h3>Windows</h3>
				<p class="req">On the roadmap</p>
				<p class="meta">The core already runs headless anywhere Rust does. A native Windows app comes after macOS.</p>
			</article>
		</div>

		{#if rel?.tar.x86_64 || rel?.tar.aarch64}
		<div class="install">
			<div>
				<h3>One line, no root.</h3>
				<p>Installs the latest Linux release into <code>~/.local</code>, checking it against the release’s SHA256SUMS first.</p>
			</div>
			<Code label="Linux" code={'curl -fsSL https://raw.githubusercontent.com/bharath914/neko-proxy/main/packaging/install.sh | sh'} />
		</div>
		{/if}

		<div class="source" use:reveal>
			<div>
				<h3>Or build it yourself.</h3>
				<p>Needs only Rust, plus Swift for the Mac app. The app starts the core for you and stops it when you quit.</p>
				<div class="seg" role="tablist" aria-label="Build from source">
					{#each source as s, i}
						<button type="button" role="tab" aria-selected={i === tab} onclick={() => (tab = i)}>{s.name}</button>
					{/each}
				</div>
			</div>
			{#key tab}
				<div class="code-in"><Code label="terminal" code={source[tab].code} /></div>
			{/key}
		</div>

		<p class="foot">
			{#if rel?.sums}<a href={rel.sums.url}>SHA256SUMS</a> · {/if}<a href={releases}>All releases</a> · <a href={repo}>Source code</a> · MIT or Apache-2.0
		</p>
	</div>
</section>

<style>
	.icon { width: 88px; height: 88px; border-radius: 20px; margin: 0 auto 22px; }
	.cards { display: grid; grid-template-columns: repeat(3, 1fr); gap: 20px; margin-top: 56px; }
	@media (max-width: 960px) { .cards { grid-template-columns: 1fr; } }
	.plat { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 12px; padding: 40px 28px 34px; min-width: 0; transition: box-shadow 0.3s; }
	.plat.mine { box-shadow: inset 0 0 0 2px var(--blue); }
	.logo { width: 40px; height: 40px; color: var(--ink); }
	.plat h3 { font-size: 1.8rem; letter-spacing: -0.026em; }
	.req { color: var(--ink-2); font-size: 0.9rem; margin-bottom: 8px; }
	.meta { font-size: 0.84rem; color: var(--ink-3); }
	.hint { font-size: 0.84rem; color: var(--ink-2); max-width: 24rem; text-align: left; }
	.hint summary { cursor: pointer; text-align: center; color: var(--blue); }
	.hint p { margin: 10px 0 8px; }
	.hint code { display: block; font-size: 0.74rem; padding: 8px 10px; user-select: all; }
	.soon { opacity: 0.85; }
	.more { width: 100%; display: grid; gap: 8px; margin-top: 6px; padding-top: 14px; border-top: 1px solid var(--line); }
	.arch { display: flex; flex-wrap: wrap; justify-content: center; align-items: baseline; gap: 6px 14px; font-size: 0.86rem; }
	.arch span { color: var(--ink-2); width: 100%; }

	.install, .source { display: grid; grid-template-columns: 1fr 1.3fr; gap: 28px 56px; align-items: center; margin-top: 72px; }
	.install > *, .source > * { min-width: 0; }
	@media (max-width: 860px) { .install, .source { grid-template-columns: 1fr; } }
	.install h3, .source h3 { font-size: 1.6rem; letter-spacing: -0.024em; margin-bottom: 8px; }
	.install p, .source p { color: var(--ink-2); }
	.seg { display: inline-flex; gap: 2px; padding: 3px; margin-top: 18px; border-radius: 999px; background: var(--fill); }
	.seg button { font: 500 0.88rem var(--font); color: var(--ink-2); background: none; border: 0; padding: 7px 16px; border-radius: 999px; cursor: pointer; transition: background-color 0.2s, color 0.2s, transform 0.25s cubic-bezier(0.2, 0.9, 0.3, 1); }
	.seg button[aria-selected='true'] { background: var(--surface); color: var(--ink); box-shadow: 0 1px 3px rgb(0 0 0 / 0.12); }
	.seg button:active { transform: scale(0.95); transition-duration: 0.1s; }
	.code-in { animation: in 0.35s cubic-bezier(0.2, 0.8, 0.2, 1); }
	@keyframes in { from { opacity: 0; } }

	.foot { margin-top: 64px; text-align: center; font-size: 0.88rem; color: var(--ink-3); }
</style>
