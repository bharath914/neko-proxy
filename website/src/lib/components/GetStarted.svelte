<script>
	import { reveal } from '#lib/reveal.js';
	import { repo, releases } from '#lib/site.js';
	import Code from './Code.svelte';

	const ways = [
		{
			id: 'linux',
			name: 'Linux',
			note: 'Needs only Rust from rustup.rs. No system packages. The app starts the core for you and stops it when you quit.',
			code: `git clone ${repo}.git
cd neko-proxy
cargo run --release -p neko-desktop
# then trust the CA: Settings > Certificate`
		},
		{
			id: 'mac',
			name: 'macOS',
			note: 'The native Mac app is in progress and builds with Swift and Rust. On first launch it makes a certificate and keeps its key in your login Keychain.',
			code: `git clone ${repo}.git
cd neko-proxy
cargo build -p neko-cli
cd desktop-macos && swift run Neko
# or package it: packaging/macos/build.sh  (writes dist/Neko.app and a .dmg)`
		},
		{
			id: 'cli',
			name: 'Headless',
			note: 'For servers, CI and scripts. Capture, list, filter and export to HAR without a window.',
			code: `cargo build --release -p neko-cli
target/release/nekod run          # proxy on 127.0.0.1:8899
target/release/nekod flows 'status:5xx host:example.com'
target/release/nekod export --har -o session.har
curl -x http://127.0.0.1:8899 --cacert "$(target/release/nekod ca path)" https://example.com`
		},
		{
			id: 'pkg',
			name: 'Packages',
			note: 'Linux packages for x86_64 and aarch64 are built by CI and will appear on the releases page with the first desktop release. Every release lists SHA256SUMS.',
			code: `sudo apt install ./neko_<ver>_amd64.deb       # Debian, Ubuntu
sudo dnf install ./neko-<ver>-1.x86_64.rpm    # Fedora, RHEL, openSUSE
chmod +x Neko-<ver>-x86_64.AppImage           # any distro
curl -fsSL https://raw.githubusercontent.com/bharath914/neko-proxy/main/packaging/install.sh | sh   # no root, into ~/.local`
		}
	];
	let active = $state(0);
	const way = $derived(ways[active]);
</script>

<section class="block start" id="start">
	<div class="wrap">
		<div class="panel" use:reveal>
			<div class="glow" aria-hidden="true"></div>
			<div class="inner">
				<div class="left">
					<span class="eyebrow">Get started</span>
					<h2 class="section-title">Up and peeking in a minute.</h2>
					<p class="section-lede">Run Neko, send traffic through <code>127.0.0.1:8899</code>, trust the certificate once. That’s the whole setup.</p>
					<div class="links">
						<a class="btn" href={repo}>
							<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M8 .2a8 8 0 0 0-2.53 15.59c.4.07.55-.17.55-.38v-1.33c-2.23.48-2.7-1.07-2.7-1.07-.36-.92-.89-1.17-.89-1.17-.73-.5.05-.49.05-.49.8.06 1.23.83 1.23.83.71 1.22 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.65-.89-3.65-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.6 7.6 0 0 1 4 0c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.28.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48v2.2c0 .21.15.46.55.38A8 8 0 0 0 8 .2Z" /></svg>
							Star on GitHub
						</a>
						<a class="btn ghost" href={releases}>Releases</a>
					</div>
				</div>
				<div class="right">
					<div class="tabs" role="tablist" aria-label="Install options">
						{#each ways as w, i}
							<button type="button" role="tab" aria-selected={i === active} onclick={() => (active = i)}>{w.name}</button>
						{/each}
					</div>
					<p class="note">{way.note}</p>
					{#key active}
						<div class="code-in"><Code code={way.code} label={way.name === 'Packages' ? 'from a release' : 'terminal'} /></div>
					{/key}
				</div>
			</div>
		</div>
	</div>
</section>

<style>
	.panel { position: relative; border-radius: 32px; overflow: hidden; background: var(--surface); border: 1px solid var(--line); box-shadow: var(--shadow-lg); }
	.glow { position: absolute; inset: 0; background: radial-gradient(50% 80% at 0% 0%, var(--glow-a), transparent 70%), radial-gradient(40% 60% at 100% 100%, var(--glow-b), transparent 70%); }
	.inner { position: relative; display: grid; grid-template-columns: 0.9fr 1.1fr; gap: 48px; padding: 56px; }
	@media (max-width: 960px) { .inner { grid-template-columns: 1fr; padding: 32px 22px; gap: 28px; } }
	.links { display: flex; gap: 10px; flex-wrap: wrap; margin-top: 28px; }
	.right { min-width: 0; }
	.tabs { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 14px; }
	.tabs button { font: inherit; font-weight: 800; font-size: 0.9rem; padding: 7px 16px; border-radius: 999px; border: 1px solid var(--line); background: var(--surface); color: var(--ink-2); cursor: pointer; transition: all 0.2s; }
	.tabs button:hover { color: var(--ink); border-color: var(--line-2); }
	.tabs button[aria-selected='true'] { background: var(--ink); color: var(--bg); border-color: var(--ink); }
	.note { color: var(--ink-2); font-size: 0.95rem; margin-bottom: 14px; min-height: 3em; }
	.code-in { animation: in 0.4s cubic-bezier(0.2, 0.8, 0.2, 1); }
	@keyframes in { from { opacity: 0; transform: translateY(6px); } }
</style>
