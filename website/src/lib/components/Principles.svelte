<script>
	import { reveal } from '#lib/reveal.js';

	const goals = [
		{ k: 'Fast', d: 'Startup and scrolling feel instant, even at 100,000 requests. The proxy adds microseconds, not milliseconds.', tone: 'blue', icon: 'M13 2 4.5 13.5H11l-1 8.5 8.5-11.5H12z' },
		{ k: 'Simple', d: 'Zero-config capture. A phone is one Gradle line or one web page away, not a tutorial.', tone: 'green', icon: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm-4 9 3 3 5-6' },
		{ k: 'Open', d: 'MIT or Apache-2.0. Your traffic, your certificate and your rules stay on your machine.', tone: 'pink', icon: 'M7 11V7.5a5 5 0 0 1 9.6-2M5.5 11h13v9.5h-13zM12 14.5v2.5' },
		{ k: 'AI-ready', d: 'An MCP server lives in the core, so a local Claude session can read traffic and propose fixes.', tone: 'purple', icon: 'M12 3.5l1.8 4.7 4.7 1.8-4.7 1.8L12 16.5l-1.8-4.7L5.5 10l4.7-1.8zM18.5 15.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8z' }
	];

	const rules = [
		['The data path never waits.', 'Requests are never held up by the UI or the disk. Saving and publishing happen off to the side.'],
		['The UI holds no logic.', 'Parsing, decoding, search and redaction live in the Rust core. Every app is a thin, swappable shell.'],
		['Your key never leaves.', 'Each install makes its own certificate authority. The private key stays in your keychain.'],
		['Captured bodies are data.', 'Traffic is untrusted input. It is redacted before AI clients see it and never treated as instructions.'],
		['Rules are files.', 'Mocks and breakpoints are plain JSON you can diff, review and commit next to your code.'],
		['Loopback by default.', 'The proxy listens on your machine only. Phones join through pairing, with a token on every connection.']
	];
</script>

<section class="block alt">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow tint">The agenda</span>
			<h2 class="section-title">Four promises,<br />kept in the code.</h2>
			<p class="section-lede">The goals every change is measured against. They’re written into the project’s design notes, and the benchmarks keep them honest.</p>
		</div>

		<div class="goals">
			{#each goals as g, i}
				<article class="goal card {g.tone}" use:reveal={i * 70}>
					<svg viewBox="0 0 24 24" aria-hidden="true"><path d={g.icon} /></svg>
					<h3>{g.k}</h3>
					<p>{g.d}</p>
				</article>
			{/each}
		</div>

		<div class="house" use:reveal>
			<h3>House rules</h3>
			<p class="sub">The constraints new work must not break.</p>
			<dl>
				{#each rules as [t, d]}
					<div><dt>{t}</dt><dd>{d}</dd></div>
				{/each}
			</dl>
		</div>
	</div>
</section>

<style>
	.goals { display: grid; grid-template-columns: repeat(4, 1fr); gap: 20px; margin-top: 56px; }
	@media (max-width: 980px) { .goals { grid-template-columns: repeat(2, 1fr); } }
	@media (max-width: 540px) { .goals { grid-template-columns: 1fr; } }
	.goal { padding: 30px 28px 32px; }
	.blue { --tone: var(--blue); } .green { --tone: var(--green); } .pink { --tone: var(--pink); } .purple { --tone: var(--purple); }
	.goal svg { width: 36px; height: 36px; fill: none; stroke: var(--tone); stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
	.goal h3 { font-size: 1.6rem; letter-spacing: -0.024em; margin: 18px 0 10px; }
	.goal p { color: var(--ink-2); font-size: 1rem; }

	.house { margin-top: 88px; }
	.house h3 { font-size: 1.6rem; letter-spacing: -0.024em; }
	.sub { color: var(--ink-2); margin-top: 6px; }
	dl { display: grid; grid-template-columns: repeat(3, 1fr); gap: 36px 40px; margin: 32px 0 0; }
	@media (max-width: 900px) { dl { grid-template-columns: 1fr 1fr; } }
	@media (max-width: 560px) { dl { grid-template-columns: 1fr; gap: 24px; } }
	dl > div { padding-top: 18px; border-top: 1px solid var(--line-2); }
	dt { font-weight: 600; letter-spacing: -0.014em; }
	dd { margin: 6px 0 0; color: var(--ink-2); font-size: 0.95rem; }
</style>
