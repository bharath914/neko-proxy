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

<section class="block alt" id="roadmap">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow tint">Roadmap</span>
			<h2 class="section-title">Built in the open,<br />one milestone at a time.</h2>
			<p class="section-lede">Capture core first, then the app, then rules and AI, then phones. Here’s where things stand.</p>
		</div>
		<div class="cols">
			{#each cols as c, i}
				<div class="col {c.tone}" use:reveal={i * 80}>
					<h3><span class="dot"></span>{c.name}</h3>
					<ul>
						{#each c.items as [t, d]}
							<li><b>{t}</b><span>{d}</span></li>
						{/each}
					</ul>
				</div>
			{/each}
		</div>
	</div>
</section>

<style>
	.cols { display: grid; grid-template-columns: repeat(3, 1fr); gap: 20px; margin-top: 64px; align-items: start; }
	@media (max-width: 900px) { .cols { grid-template-columns: 1fr; } }
	.col { padding: 28px; border-radius: var(--r-lg); background: var(--surface); }
	.green { --tone: var(--green); } .amber { --tone: var(--amber); } .blue { --tone: var(--blue); }
	h3 { display: flex; align-items: center; gap: 10px; font-size: 1.3rem; margin-bottom: 8px; }
	.dot { width: 10px; height: 10px; border-radius: 50%; background: var(--tone); }
	ul { list-style: none; margin: 0; padding: 0; }
	li { display: flex; flex-direction: column; gap: 3px; padding: 14px 0; border-top: 1px solid var(--line); }
	li:first-child { border-top: 0; }
	li b { font-weight: 600; letter-spacing: -0.014em; }
	li span { color: var(--ink-2); font-size: 0.92rem; }
</style>
