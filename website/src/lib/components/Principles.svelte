<script>
	import { reveal } from '#lib/reveal.js';

	const goals = [
		{ k: 'Fast', d: 'Startup and scrolling should feel instant, even at 100,000 requests. The proxy adds microseconds, not milliseconds.', c: 'blue', e: '⚡' },
		{ k: 'Simple', d: 'Zero-config capture. A phone should be one QR code or one Gradle line away, not a tutorial.', c: 'green', e: '🧶' },
		{ k: 'Open', d: 'MIT or Apache-2.0. Your traffic, your certificate and your rules stay on your machine as plain files.', c: 'pink', e: '🔓' },
		{ k: 'AI-ready', d: 'An MCP server lives in the core, so any local Claude session can read traffic and propose fixes.', c: 'purple', e: '🤖' }
	];

	const rules = [
		['The data path never waits.', 'Requests are never held up by the UI or the disk. Saving and publishing happen off to the side.'],
		['The UI holds no logic.', 'Parsing, decoding, search and redaction live in the Rust core. Every app is a thin, swappable shell.'],
		['Your key never leaves.', 'Each install makes its own certificate authority. The private key stays in your keychain.'],
		['Captured bodies are data.', 'Traffic is untrusted input. It is redacted before AI clients see it and never treated as instructions.'],
		['Rules are files.', 'Mocks and breakpoints are JSON you can diff, review and commit next to your code.'],
		['Loopback by default.', 'The proxy listens on your machine only. Phones join through pairing, with a token on every connection.']
	];
</script>

<section class="block principles">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow">The agenda</span>
			<h2 class="section-title">Four promises, kept in the code.</h2>
			<p class="section-lede">These are the goals every change is measured against. They’re written into the project’s design notes, and the benchmarks keep them honest.</p>
		</div>

		<div class="goals">
			{#each goals as g, i}
				<article class="goal card {g.c}" use:reveal={i * 80}>
					<span class="emoji" aria-hidden="true">{g.e}</span>
					<h3>{g.k}</h3>
					<p>{g.d}</p>
				</article>
			{/each}
		</div>

		<div class="house card" use:reveal>
			<h3>House rules <span>· the constraints new work must not break</span></h3>
			<dl>
				{#each rules as [t, d]}
					<div><dt>{t}</dt><dd>{d}</dd></div>
				{/each}
			</dl>
		</div>
	</div>
</section>

<style>
	.principles { background: linear-gradient(180deg, transparent, var(--bg-2) 30%, var(--bg-2) 70%, transparent); }
	.goals { display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; margin-top: 48px; }
	@media (max-width: 960px) { .goals { grid-template-columns: repeat(2, 1fr); } }
	@media (max-width: 520px) { .goals { grid-template-columns: 1fr; } }
	.goal { position: relative; padding: 26px 24px 28px; overflow: hidden; transition: transform 0.3s cubic-bezier(0.2, 0.8, 0.2, 1), box-shadow 0.3s; }
	.goal:hover { transform: translateY(-4px); box-shadow: var(--shadow); }
	.goal::before { content: ''; position: absolute; inset: 0 0 auto; height: 4px; background: var(--tone); }
	.goal::after { content: ''; position: absolute; right: -40px; top: -40px; width: 140px; height: 140px; border-radius: 50%; background: var(--tone); opacity: 0.08; }
	.blue { --tone: var(--blue); } .green { --tone: var(--green); } .pink { --tone: var(--pink); } .purple { --tone: var(--purple); }
	.emoji { font-size: 1.9rem; line-height: 1; }
	.goal h3 { font-size: 1.5rem; margin: 16px 0 8px; color: var(--tone); }
	.goal p { color: var(--ink-2); font-size: 0.98rem; }

	.house { margin-top: 20px; padding: 30px 32px; }
	.house h3 { font-size: 1.2rem; }
	.house h3 span { color: var(--ink-3); font-weight: 700; font-size: 0.95rem; }
	dl { display: grid; grid-template-columns: repeat(3, 1fr); gap: 22px 32px; margin: 22px 0 0; }
	@media (max-width: 900px) { dl { grid-template-columns: 1fr 1fr; } }
	@media (max-width: 560px) { dl { grid-template-columns: 1fr; } .house { padding: 24px 20px; } }
	dt { font-weight: 800; }
	dt::before { content: '✓'; color: var(--green); font-weight: 900; margin-right: 8px; }
	dd { margin: 4px 0 0; color: var(--ink-2); font-size: 0.94rem; }
</style>
