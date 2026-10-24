<script>
	import { resolve } from '$app/paths';
	import { repo, releases } from '#lib/site.js';
</script>

<footer>
	<div class="wrap">
		<p class="about"><b>Neko Proxy</b> is a fast, open-source HTTP and HTTPS debugging proxy. <i>Neko</i> is Japanese for cat: it sits quietly on your network and peeks at everything.</p>
		<div class="grid">
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
		<div class="legal">
			<span>MIT or Apache-2.0, your choice.</span>
			<span>Made with 🐾 and Rust.</span>
		</div>
	</div>
</footer>

<style>
	footer { background: var(--bg-alt); padding: 40px 0 28px; font-size: 0.8rem; color: var(--ink-2); letter-spacing: -0.005em; }
	.about { padding-bottom: 18px; border-bottom: 1px solid var(--line); max-width: none; }
	.about b { color: var(--ink); font-weight: 600; }
	.grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 24px; padding: 22px 0; }
	@media (max-width: 734px) { .grid { grid-template-columns: 1fr 1fr; } }
	.grid div { display: flex; flex-direction: column; gap: 8px; }
	h4 { font-size: 0.8rem; font-weight: 600; color: var(--ink); margin-bottom: 2px; }
	.grid a { color: var(--ink-2); }
	.grid a:hover { color: var(--ink); }
	.legal { display: flex; justify-content: space-between; flex-wrap: wrap; gap: 8px; padding-top: 16px; border-top: 1px solid var(--line); color: var(--ink-3); }
</style>
