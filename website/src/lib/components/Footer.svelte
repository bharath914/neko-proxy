<script>
	import { asset, resolve } from '$app/paths';
	import { repo, releases } from '#lib/site.js';
</script>

<footer>
	<div class="wrap grid">
		<div class="about">
			<a class="brand" href={resolve('/')}>
				<img src={asset('/assets/neko.png')} alt="" width="40" height="40" />
				<span>Neko Proxy</span>
			</a>
			<p>A fast, open-source HTTP and HTTPS debugging proxy. <i>Neko</i> is Japanese for cat: it sits quietly on your network and peeks at everything.</p>
		</div>
		<div>
			<h4>Project</h4>
			<a href={repo}>Source code</a>
			<a href={releases}>Releases</a>
			<a href={`${repo}/issues`}>Issues</a>
			<a href={`${repo}/blob/main/CLAUDE.md`}>Design notes</a>
		</div>
		<div>
			<h4>Guides</h4>
			<a href={resolve('/install-cert')}>Install the certificate</a>
			<a href={`${repo}/blob/main/docs/sdk-quickstart.md`}>Android SDK quickstart</a>
			<a href={`${repo}/blob/main/docs/phase-2.md`}>Rules and MCP</a>
			<a href={`${repo}/blob/main/docs/bench.md`}>Benchmarks</a>
		</div>
	</div>
	<div class="wrap legal">
		<span>MIT or Apache-2.0, your choice.</span>
		<span>Made with 🐾 and Rust.</span>
	</div>
</footer>

<style>
	footer { border-top: 1px solid var(--line); background: var(--bg-2); padding: 56px 0 32px; margin-top: 40px; }
	.grid { display: grid; grid-template-columns: 2fr 1fr 1fr; gap: 40px; }
	@media (max-width: 720px) { .grid { grid-template-columns: 1fr 1fr; } .about { grid-column: 1 / -1; } }
	.brand { display: inline-flex; align-items: center; gap: 12px; color: var(--ink); text-decoration: none; font-weight: 900; font-size: 1.2rem; margin-bottom: 12px; }
	.brand img { border-radius: 11px; }
	.about p { color: var(--ink-2); max-width: 26rem; font-size: 0.96rem; }
	h4 { font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.12em; color: var(--ink-3); margin-bottom: 12px; font-weight: 800; }
	.grid > div:not(.about) { display: flex; flex-direction: column; gap: 8px; }
	.grid a:not(.brand) { color: var(--ink-2); text-decoration: none; font-weight: 600; font-size: 0.95rem; }
	.grid a:not(.brand):hover { color: var(--ink); }
	.legal { display: flex; justify-content: space-between; gap: 12px; flex-wrap: wrap; margin-top: 44px; padding-top: 20px; border-top: 1px solid var(--line); color: var(--ink-3); font-size: 0.88rem; }
</style>
