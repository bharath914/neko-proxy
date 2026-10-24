<script>
	import { reveal } from '#lib/reveal.js';

	const cols = [
		{
			name: 'Shipped',
			tone: 'green',
			items: [
				['Rust capture core', 'HTTP/1.1, HTTP/2, WebSocket, SOCKS5, TLS interception, pinning detection'],
				['Linux desktop app', '.deb, .rpm, AppImage and a no-root installer, x86_64 and aarch64'],
				['Rules and breakpoints', 'Mock, map remote, map local, rewrite, throttle, replay'],
				['MCP server', 'Read tools, approved writes, redaction'],
				['Headless CLI', 'Run, list flows, HAR export, CA tools'],
				['Android SDK', 'Gradle plugin, auto-pairing, runtime CA trust']
			]
		},
		{
			name: 'In progress',
			tone: 'amber',
			items: [
				['macOS app', 'Native AppKit: Inspector, Rules, Breakpoints, Devices, Settings and a command palette are built'],
				['iOS SDK', 'Swift package for URLSession, partly verified'],
				['Device testing', 'End-to-end runs of the SDKs on real phones and emulators']
			]
		},
		{
			name: 'Next',
			tone: 'blue',
			items: [
				['Any phone over Wi-Fi', 'LAN proxy mode with QR pairing, no SDK needed'],
				['Windows app', 'Same core, native shell'],
				['HTTP/3', 'QUIC capture'],
				['Scripting', 'JS or Lua hooks on requests and responses'],
				['MCP for SDK data', 'Let AI clients see in-app traffic and routes']
			]
		}
	];
</script>

<section class="block" id="roadmap">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow">Roadmap</span>
			<h2 class="section-title">Built in the open, one milestone at a time.</h2>
			<p class="section-lede">Capture core first, then the app, then rules and AI, then phones. Here’s where things stand.</p>
		</div>
		<div class="cols">
			{#each cols as c, i}
				<div class="col {c.tone}" use:reveal={i * 90}>
					<h3><span class="dot"></span>{c.name}<span class="count">{c.items.length}</span></h3>
					<ul>
						{#each c.items as [t, d]}
							<li class="card"><b>{t}</b><span>{d}</span></li>
						{/each}
					</ul>
				</div>
			{/each}
		</div>
	</div>
</section>

<style>
	.cols { display: grid; grid-template-columns: repeat(3, 1fr); gap: 20px; margin-top: 48px; align-items: start; }
	@media (max-width: 900px) { .cols { grid-template-columns: 1fr; } }
	.col { padding: 16px; border-radius: var(--r-lg); background: var(--bg-2); border: 1px solid var(--line); }
	.green { --tone: var(--green); } .amber { --tone: var(--amber); } .blue { --tone: var(--blue); }
	h3 { display: flex; align-items: center; gap: 10px; font-size: 1.05rem; padding: 4px 6px 14px; }
	.dot { width: 10px; height: 10px; border-radius: 50%; background: var(--tone); box-shadow: 0 0 0 4px color-mix(in srgb, var(--tone) 20%, transparent); }
	.count { margin-left: auto; font: 600 0.78rem var(--mono); color: var(--ink-3); }
	ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
	li { padding: 14px 16px; display: flex; flex-direction: column; gap: 3px; border-left: 3px solid var(--tone); }
	li b { font-weight: 800; }
	li span { color: var(--ink-2); font-size: 0.88rem; }
</style>
