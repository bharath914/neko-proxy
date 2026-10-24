<script>
	import { reveal } from '#lib/reveal.js';

	const rows = [
		['Cost', 'The proxy everyone uses is a paid, closed-source Java app.', 'Free and open source under MIT or Apache-2.0. Read it, fork it, ship it inside your team.'],
		['Speed', 'Big sessions make the window stutter, and it takes a while just to start.', 'A Rust core that is ready in 18 ms, and a native app that draws its first frame in about 200 ms.'],
		['Phones', 'Getting a phone to trust the certificate is a scavenger hunt through settings.', 'One page at neko.local walks each platform through it. The in-app SDK skips it for your own app.'],
		['Mocks', 'Mocks and breakpoints feel bolted on, buried in dialogs.', 'Mock, map, rewrite, throttle and break are first-class rules, one click from any request.'],
		['AI', 'Your assistant can’t see the traffic you’re staring at.', 'A built-in MCP server lets Claude search flows and propose rules, with secrets redacted and your approval required.']
	];
</script>

<section class="block" id="why">
	<div class="wrap">
		<div class="head" use:reveal>
			<span class="eyebrow tint">Why Neko exists</span>
			<h2 class="section-title">Debugging the network<br />shouldn’t feel like a chore.</h2>
			<p class="section-lede">
				Every app talks to a server, and when something breaks, the answer is almost always in the traffic. The
				tools we reached for were slow, closed and paid, phones were a scavenger hunt, and our AI assistants were
				blind to the wire. <b>So we built the proxy we wanted: fast enough to forget about, simple enough to set
				up in a minute, open to everyone, and ready for AI from day one.</b>
			</p>
		</div>

		<div class="table" use:reveal={80}>
			<div class="row th" aria-hidden="true"><span></span><span>Before</span><span>With Neko</span></div>
			{#each rows as [k, was, now]}
				<div class="row">
					<span class="k">{k}</span>
					<p class="was">{was}</p>
					<p class="now">{now}</p>
				</div>
			{/each}
		</div>
	</div>
</section>

<style>
	.head { max-width: 50rem; }
	.table { margin-top: 64px; }
	.row { display: grid; grid-template-columns: 9rem 1fr 1fr; gap: 32px; padding: 26px 0; border-top: 1px solid var(--line); }
	.row.th { padding: 0 0 12px; border-top: 0; font-size: 0.82rem; font-weight: 600; color: var(--ink-3); }
	.k { font-weight: 600; font-size: 1.05rem; letter-spacing: -0.016em; }
	.was { color: var(--ink-3); }
	.now { color: var(--ink); font-weight: 500; }
	@media (max-width: 860px) {
		.row { grid-template-columns: 1fr; gap: 6px; padding: 22px 0; }
		.row.th { display: none; }
		.was::before { content: 'Before: '; font-weight: 600; }
	}
</style>
