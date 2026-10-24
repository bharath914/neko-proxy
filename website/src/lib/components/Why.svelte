<script>
	import { reveal } from '#lib/reveal.js';

	const pains = [
		{
			was: 'The proxy everyone uses is a paid, closed-source Java app.',
			now: 'Neko is free and open source under MIT or Apache-2.0. Read it, fork it, ship it inside your team.',
			icon: 'M7 11V7a5 5 0 0 1 9.9-1M5 11h14v10H5z'
		},
		{
			was: 'Big sessions make the window stutter, and it takes a while just to start.',
			now: 'A Rust core that is ready in 18 ms and a native app that draws its first frame in about 200 ms.',
			icon: 'M13 2 4 14h7l-1 8 9-12h-7z'
		},
		{
			was: 'Getting a phone to trust the certificate is a scavenger hunt through settings.',
			now: 'One page at neko.local walks each platform through it, and the in-app SDK skips it entirely for your own app.',
			icon: 'M7 2h10a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zM11 18h2'
		},
		{
			was: 'Mocks and breakpoints feel bolted on, buried in dialogs.',
			now: 'Mock, map, rewrite, throttle and break are first-class rules: one click from any request, saved as plain JSON.',
			icon: 'M4 6h16M4 12h10M4 18h6'
		},
		{
			was: 'Your AI assistant can’t see the traffic you’re staring at.',
			now: 'A built-in MCP server lets Claude and other clients search flows and propose rules, with secrets redacted and your approval required.',
			icon: 'M12 3l1.9 4.6L18.5 9l-4.6 1.9L12 15.5l-1.9-4.6L5.5 9l4.6-1.4zM19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z'
		}
	];
</script>

<section class="block" id="why">
	<div class="wrap grid">
		<div class="story" use:reveal>
			<span class="eyebrow">Why Neko exists</span>
			<h2 class="section-title">Debugging the network shouldn’t feel like a chore.</h2>
			<div class="prose">
				<p>
					Every app talks to a server, and when something breaks, the answer is almost always in the traffic. A
					debugging proxy is how you see it: it sits between your app and the internet and shows you every
					request and response.
				</p>
				<p>
					The tools we reached for were slow, closed and paid, and the hardest part, getting a phone to trust the
					proxy, was left to a dozen settings screens. Meanwhile the way we work changed: half our debugging now
					happens next to an AI assistant that has no idea what’s on the wire.
				</p>
				<p class="strong">So we built the proxy we wanted: fast enough to forget about, simple enough to set up in a minute, open to everyone, and ready for AI from day one.</p>
			</div>
		</div>

		<ol class="pains">
			{#each pains as p, i}
				<li class="card" use:reveal={i * 70}>
					<span class="ico"><svg viewBox="0 0 24 24" aria-hidden="true"><path d={p.icon} /></svg></span>
					<div>
						<p class="was">{p.was}</p>
						<p class="now">{p.now}</p>
					</div>
				</li>
			{/each}
		</ol>
	</div>
</section>

<style>
	.grid { display: grid; grid-template-columns: 1fr 1.1fr; gap: 64px; align-items: start; }
	@media (max-width: 900px) { .grid { grid-template-columns: 1fr; gap: 36px; } }
	.story { position: sticky; top: 100px; }
	@media (max-width: 900px) { .story { position: static; } }
	.prose { display: grid; gap: 16px; color: var(--ink-2); font-size: 1.08rem; }
	.prose .strong { color: var(--ink); font-weight: 700; padding-left: 16px; border-left: 3px solid var(--pink); }
	.pains { list-style: none; margin: 0; padding: 0; display: grid; gap: 14px; }
	.pains li { display: grid; grid-template-columns: 44px 1fr; gap: 16px; padding: 20px 22px; transition: transform 0.25s, box-shadow 0.25s, border-color 0.25s; }
	.pains li:hover { transform: translateX(4px); box-shadow: var(--shadow); border-color: var(--line-2); }
	.ico { width: 44px; height: 44px; border-radius: 12px; display: grid; place-items: center; background: var(--blue-soft); color: var(--blue); }
	.ico svg { width: 22px; height: 22px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
	.was { color: var(--ink-3); font-size: 0.95rem; text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--pink) 60%, transparent); text-decoration-thickness: 1.5px; }
	.now { color: var(--ink); font-weight: 700; margin-top: 6px; }
</style>
