<script>
	import { reveal } from '#lib/reveal.js';

	const stages = [
		['Accept', 'A connection arrives on 127.0.0.1:8899: plain HTTP, CONNECT or SOCKS5.'],
		['Intercept TLS', 'A leaf certificate is minted from your own CA. Apps that pin are tunneled untouched and flagged.'],
		['Parse', 'HTTP/1.1, HTTP/2 or WebSocket, with bounded buffers and fuzzed parsers.'],
		['Request rules', 'Breakpoint, map, mock or rewrite, in the order you listed them.'],
		['Forward', 'Sent upstream through any throttle, timing DNS, connect, TLS and wait.'],
		['Response rules', 'Rewrite or hold the response before your app sees it.'],
		['Persist', 'SQLite metadata and zstd-compressed bodies, written off the hot path.'],
		['Publish', 'A small delta goes to every subscriber: the app, the CLI, your AI client.']
	];

	const crates = [
		['net', 'listeners, HTTP/1.1, HTTP/2, WebSocket'],
		['tls', 'CA, leaf cache, pinning detection'],
		['flow', 'flow model and pipeline'],
		['rules', 'mock, map, rewrite, throttle, break'],
		['store', 'SQLite WAL, zstd blobs, FTS'],
		['decode', 'gzip, br, zstd, JSON, images'],
		['api', 'WebSocket + protobuf'],
		['mcp', 'MCP server and redaction'],
		['cli', 'headless runs, HAR export']
	];
</script>

<section class="block life" id="how">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow">Under the hood</span>
			<h2 class="section-title">The life of a request, in eight steps.</h2>
			<p class="section-lede">The data path never waits on the UI or the disk. Your app gets its response first; Neko writes things down afterwards.</p>
		</div>

		<div class="track" use:reveal>
			<div class="rail" aria-hidden="true"><span class="packet"></span><span class="packet late"></span></div>
			<ol>
				{#each stages as [name, text], i}
					<li style="--i:{i}">
						<span class="node">{i + 1}</span>
						<h3>{name}</h3>
						<p>{text}</p>
					</li>
				{/each}
			</ol>
		</div>

		<div class="arch" use:reveal>
			<div class="layer clients">
				<span class="lbl">Clients</span>
				<div class="row">
					<span class="box">🖥️ Desktop app<small>Rust + egui on Linux, AppKit on macOS</small></span>
					<span class="box">📱 In-app SDK<small>Android, iOS in progress</small></span>
					<span class="box">🤖 AI clients<small>MCP over loopback HTTP</small></span>
					<span class="box">⌨️ CLI<small>nekod flows, export, ca</small></span>
				</div>
			</div>
			<div class="pipe" aria-hidden="true"><span>WebSocket + protobuf · token on every connection</span></div>
			<div class="layer core">
				<span class="lbl"><b>nekod</b> · the Rust core</span>
				<div class="crates">
					{#each crates as [c, d]}
						<span class="crate"><code>neko-{c}</code><small>{d}</small></span>
					{/each}
				</div>
			</div>
			<div class="pipe" aria-hidden="true"><span>HTTP/1.1 · HTTP/2 · WebSocket</span></div>
			<div class="layer net">
				<span class="lbl">Your apps, browsers and phones ⇄ the internet</span>
			</div>
		</div>
	</div>
</section>

<style>
	.life { overflow: hidden; }
	.track { position: relative; margin-top: 56px; }
	ol { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(8, 1fr); gap: 12px; position: relative; }
	.rail { position: absolute; left: calc(100% / 16); right: calc(100% / 16); top: 21px; height: 3px; border-radius: 3px; background: linear-gradient(90deg, var(--blue), var(--pink)); opacity: 0.9; }
	.packet { position: absolute; top: -5px; left: 0; width: 13px; height: 13px; border-radius: 50%; background: var(--surface); border: 3px solid var(--blue); box-shadow: 0 0 0 6px color-mix(in srgb, var(--blue) 18%, transparent); animation: travel 6s cubic-bezier(0.65, 0, 0.35, 1) infinite; }
	.packet.late { animation-delay: -3s; border-color: var(--pink); box-shadow: 0 0 0 6px color-mix(in srgb, var(--pink) 18%, transparent); }
	@keyframes travel { 0% { left: 0; opacity: 0; } 6% { opacity: 1; } 94% { opacity: 1; } 100% { left: calc(100% - 13px); opacity: 0; } }
	li { position: relative; text-align: center; }
	.node { position: relative; z-index: 1; display: grid; place-items: center; width: 44px; height: 44px; margin: 0 auto 14px; border-radius: 50%; background: var(--surface); border: 2px solid var(--line-2); font: 600 0.9rem var(--mono); color: var(--ink-2); box-shadow: var(--shadow-sm); animation: blink 6s infinite; animation-delay: calc(var(--i) * 0.68s); }
	@keyframes blink { 0%, 10% { border-color: var(--line-2); color: var(--ink-2); } 3%, 6% { border-color: var(--blue); color: var(--blue); } }
	li h3 { font-size: 0.98rem; margin-bottom: 6px; }
	li p { font-size: 0.82rem; color: var(--ink-2); line-height: 1.5; }

	@media (max-width: 960px) {
		ol { grid-template-columns: 1fr; gap: 0; padding-left: 4px; }
		.rail { left: 25px; right: auto; top: 22px; bottom: 22px; width: 3px; height: auto; background: linear-gradient(180deg, var(--blue), var(--pink)); }
		.packet { left: -5px !important; animation-name: travel-v; }
		@keyframes travel-v { 0% { top: 0; opacity: 0; } 6% { opacity: 1; } 94% { opacity: 1; } 100% { top: calc(100% - 13px); opacity: 0; } }
		li { display: grid; grid-template-columns: 44px 1fr; column-gap: 16px; text-align: left; padding-bottom: 20px; }
		.node { grid-row: span 2; margin: 0; }
		li h3 { align-self: end; margin: 0; }
	}

	.arch { margin-top: 72px; padding: 28px; border-radius: var(--r-lg); background: var(--surface); border: 1px solid var(--line); box-shadow: var(--shadow); }
	@media (max-width: 560px) { .arch { padding: 18px; } }
	.layer { border-radius: var(--r); padding: 18px; }
	.lbl { display: block; font-size: 0.78rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; color: var(--ink-3); margin-bottom: 12px; }
	.lbl b { color: var(--blue); }
	.clients { background: var(--surface-2); }
	.row { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }
	@media (max-width: 760px) { .row { grid-template-columns: 1fr 1fr; } }
	.box { display: flex; flex-direction: column; gap: 2px; padding: 12px 14px; border-radius: 12px; background: var(--surface); border: 1px solid var(--line); font-weight: 800; font-size: 0.95rem; }
	.box small, .crate small { font-weight: 600; font-size: 0.78rem; color: var(--ink-3); }
	.pipe { position: relative; height: 46px; display: grid; place-items: center; }
	.pipe::before { content: ''; position: absolute; top: 0; bottom: 0; left: 50%; width: 2px; background: repeating-linear-gradient(180deg, var(--line-2) 0 6px, transparent 6px 11px); animation: dash 1s linear infinite; }
	@keyframes dash { to { background-position: 0 11px; } }
	.pipe span { position: relative; background: var(--surface); padding: 2px 12px; font: 500 0.74rem var(--mono); color: var(--ink-3); border-radius: 999px; border: 1px solid var(--line); text-align: center; }
	.core { background: linear-gradient(135deg, color-mix(in srgb, var(--blue) 12%, var(--surface)), color-mix(in srgb, var(--pink) 8%, var(--surface))); border: 1px solid color-mix(in srgb, var(--blue) 30%, var(--line)); }
	.crates { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
	@media (max-width: 760px) { .crates { grid-template-columns: 1fr 1fr; } }
	.crate { display: flex; flex-direction: column; gap: 2px; padding: 10px 12px; border-radius: 10px; background: color-mix(in srgb, var(--surface) 85%, transparent); border: 1px solid var(--line); }
	.crate code { background: none; border: 0; padding: 0; font-size: 0.86rem; font-weight: 600; color: var(--ink); }
	.net { text-align: center; background: var(--surface-2); padding: 14px; }
	.net .lbl { margin: 0; }
</style>
