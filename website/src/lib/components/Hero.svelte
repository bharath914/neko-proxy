<script>
	import { onMount } from 'svelte';
	import { flip } from 'svelte/animate';
	import { fly } from 'svelte/transition';
	import { asset } from '$app/paths';
	import { repo } from '#lib/site.js';
	import Shot from './Shot.svelte';

	// Requests from the capture behind the screenshots on this page.
	const feed = [
		['GET', 'dummyjson.com', '/products/search?q=phone', 200, '313 ms'],
		['POST', 'dummyjson.com', '/carts/add', 503, '301 ms', 'mocked'],
		['GET', 'api.github.com', '/repos/rust-lang/rust', 200, '554 ms'],
		['GET', 'pokeapi.co', '/api/v2/pokemon/pikachu', 200, '53 ms'],
		['PUT', 'dummyjson.com', '/carts/1', 200, '288 ms'],
		['GET', 'httpbin.org', '/status/500', 500, '1.07 s'],
		['PATCH', 'jsonplaceholder.typicode.com', '/posts/1', 200, '345 ms'],
		['GET', 'cdn.dummyjson.com', '/products/images/…/thumbnail.png', 200, '1.87 s', 'throttled'],
		['POST', 'dummyjson.com', '/auth/login', 0, 'held', 'breakpoint'],
		['GET', 'api.open-meteo.com', '/v1/forecast?latitude=12.97…', 200, '728 ms'],
		['DELETE', 'dummyjson.com', '/products/1', 200, '311 ms'],
		['GET', 'registry.npmjs.org', '/svelte/latest', 200, '307 ms']
	];
	let next = 4;
	let rows = $state(feed.slice(0, 4).map((r, i) => ({ id: i, r })));

	onMount(() => {
		if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		const t = setInterval(() => {
			const r = feed[next % feed.length];
			rows = [{ id: next, r }, ...rows].slice(0, 4);
			next++;
		}, 1700);
		return () => clearInterval(t);
	});

	const tone = (s) => (s === 0 ? 'held' : s >= 500 ? 'bad' : s >= 400 ? 'warn' : 'ok');
</script>

<section class="hero">
	<div class="glow" aria-hidden="true"></div>
	<div class="wrap">
		<div class="intro">
			<div class="copy">
				<a class="badge" href={repo}>
					<span class="new">Open source</span>
					<span>Rust core · native apps · MCP built in</span>
					<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3l5 5-5 5" /></svg>
				</a>
				<h1>See every request.<br /><span class="grad">Change any of them.</span></h1>
				<p class="lede">
					Neko is a fast, free HTTP and HTTPS debugging proxy. Inspect traffic from your browser, terminal or phone,
					mock and pause requests mid-flight, and let your AI assistant read along, without paying for a licence
					or fighting certificate settings.
				</p>
				<div class="cta">
					<a class="btn" href="#start">
						<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v11m0 0l-4.5-4.5M12 15l4.5-4.5M5 19.5h14" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" /></svg>
						Get Neko
					</a>
					<a class="btn ghost" href="#why">Why we built it</a>
				</div>
				<p class="meta">Linux today · macOS app in progress · MIT or Apache-2.0</p>
			</div>
			<div class="mascot" aria-hidden="true">
				<div class="halo"></div>
				<img src={asset('/assets/neko.png')} alt="" width="220" height="220" />
				<span class="paw p1">🐾</span><span class="paw p2">🐾</span>
			</div>
		</div>

		<div class="stage">
			<Shot name="inspector" eager alt="The Neko Inspector showing 87 captured requests, with a JSON product response open in a tree view" />

			<div class="ticker" aria-hidden="true">
				<div class="tk-head"><span class="rec"></span> Recording <span class="tk-port">127.0.0.1:8899</span></div>
				<div class="tk-rows">
				{#each rows as row (row.id)}
					<div class="tk-row" animate:flip={{ duration: 400 }} in:fly={{ y: -14, duration: 400 }}>
						<span class="m m-{row.r[0].toLowerCase()}">{row.r[0]}</span>
						<span class="u"><b>{row.r[1]}</b>{row.r[2]}</span>
						<span class="s {tone(row.r[3])}">{row.r[3] || '⏸'}</span>
						{#if row.r[5]}<span class="tag">{row.r[5]}</span>{:else}<span class="t">{row.r[4]}</span>{/if}
					</div>
				{/each}
				</div>
			</div>

			<div class="chip c1"><b>18 ms</b> core cold start</div>
			<div class="chip c2"><span class="ai">AI</span> asks before it changes anything</div>
		</div>
		<p class="caption">Real traffic, captured by Neko on a Mac: GitHub, PokéAPI, dummyjson and friends, with one mock, one throttle and one breakpoint switched on.</p>
	</div>
</section>

<style>
	.hero { position: relative; padding: 40px 0 72px; overflow: hidden; }
	.glow {
		position: absolute;
		inset: -20% -10% auto;
		height: 900px;
		background:
			radial-gradient(40% 50% at 20% 25%, var(--glow-a), transparent 70%),
			radial-gradient(35% 45% at 85% 20%, var(--glow-b), transparent 70%),
			radial-gradient(50% 40% at 50% 80%, var(--glow-a), transparent 70%);
		filter: blur(10px);
		pointer-events: none;
	}
	.glow::after {
		content: '';
		position: absolute;
		inset: 0;
		background-image: radial-gradient(color-mix(in srgb, var(--ink) 9%, transparent) 1px, transparent 1px);
		background-size: 22px 22px;
		mask-image: radial-gradient(60% 50% at 50% 30%, #000, transparent 75%);
		-webkit-mask-image: radial-gradient(60% 50% at 50% 30%, #000, transparent 75%);
	}
	.intro { position: relative; display: grid; grid-template-columns: 1fr auto; gap: 40px; align-items: center; padding-top: 32px; }
	@media (max-width: 860px) { .intro { grid-template-columns: 1fr; } .intro .mascot { display: none; } }

	.badge {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		padding: 5px 12px 5px 5px;
		border-radius: 999px;
		background: var(--surface);
		border: 1px solid var(--line);
		box-shadow: var(--shadow-sm);
		color: var(--ink-2);
		text-decoration: none;
		font-size: 0.88rem;
		font-weight: 700;
	}
	.badge .new { flex: none; white-space: nowrap; background: var(--blue); color: var(--on-blue); border-radius: 999px; padding: 2px 10px; font-weight: 800; font-size: 0.8rem; }
	@media (max-width: 480px) { .badge { font-size: 0.8rem; } }
	.badge svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 2; }
	.badge:hover { color: var(--ink); }

	h1 { font-size: clamp(2.6rem, 7vw, 4.9rem); line-height: 1.02; letter-spacing: -0.035em; margin: 22px 0 22px; }
	.lede { font-size: clamp(1.08rem, 2vw, 1.25rem); color: var(--ink-2); max-width: 38rem; }
	.cta { display: flex; gap: 12px; flex-wrap: wrap; margin: 30px 0 16px; }
	.meta { color: var(--ink-3); font-size: 0.9rem; font-weight: 600; }

	.mascot { position: relative; width: 260px; height: 260px; display: grid; place-items: center; }
	.mascot img { position: relative; width: 220px; height: 220px; border-radius: 52px; box-shadow: var(--shadow-lg); animation: bob 5s ease-in-out infinite; }
	.halo { position: absolute; inset: 0; border-radius: 50%; background: conic-gradient(from 0deg, var(--blue), var(--pink), var(--blue)); filter: blur(48px); opacity: 0.35; animation: spin 14s linear infinite; }
	.paw { position: absolute; font-size: 1.3rem; opacity: 0.55; }
	.p1 { left: -6px; bottom: 30px; transform: rotate(-20deg); }
	.p2 { right: 0; top: 18px; transform: rotate(25deg); }
	@keyframes bob { 50% { transform: translateY(-10px) rotate(-2deg); } }
	@keyframes spin { to { transform: rotate(360deg); } }

	.stage { position: relative; margin-top: 64px; perspective: 1600px; }
	.stage :global(.window) { transform: rotateX(6deg); transform-origin: 50% 0; animation: rise 1.2s 0.1s cubic-bezier(0.2, 0.8, 0.2, 1) both; }
	@keyframes rise { from { opacity: 0; transform: rotateX(18deg) translateY(40px); } }

	.ticker {
		position: absolute;
		right: -12px;
		bottom: -36px;
		width: min(440px, 62%);
		padding: 12px;
		border-radius: 16px;
		background: color-mix(in srgb, var(--surface) 97%, transparent);
		backdrop-filter: blur(18px) saturate(1.5);
		-webkit-backdrop-filter: blur(18px) saturate(1.5);
		border: 1px solid var(--line-2);
		box-shadow: var(--shadow-lg);
		font-size: 0.8rem;
		overflow: hidden;
	}
	.tk-head { display: flex; align-items: center; gap: 8px; font-weight: 800; color: var(--ink-2); padding: 0 4px 8px; }
	.rec { width: 8px; height: 8px; border-radius: 50%; background: var(--red); box-shadow: 0 0 0 4px color-mix(in srgb, var(--red) 20%, transparent); animation: pulse 1.6s ease-in-out infinite; }
	@keyframes pulse { 50% { box-shadow: 0 0 0 7px color-mix(in srgb, var(--red) 0%, transparent); } }
	.tk-port { margin-left: auto; font: 500 0.74rem var(--mono); color: var(--ink-3); }
	.tk-rows { min-height: 152px; }
	.tk-row { display: grid; grid-template-columns: 54px 1fr auto 62px; gap: 8px; align-items: center; padding: 7px 6px; border-radius: 9px; font-family: var(--mono); }
	.tk-row:first-of-type { background: var(--blue-soft); }
	.m { font-weight: 600; font-size: 0.72rem; color: var(--green); }
	.m-post { color: var(--amber); } .m-put { color: var(--blue); } .m-patch { color: var(--purple); } .m-delete { color: var(--red); }
	.u { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--ink-3); font-size: 0.74rem; }
	.u b { color: var(--ink); font-weight: 500; }
	.s { font-weight: 600; font-size: 0.74rem; }
	.s.ok { color: var(--green); } .s.warn { color: var(--amber); } .s.bad { color: var(--red); } .s.held { color: var(--amber); }
	.t { text-align: right; color: var(--ink-3); font-size: 0.72rem; }
	.tag { justify-self: end; font: 800 0.66rem var(--font); padding: 1px 7px; border-radius: 999px; background: var(--pink-soft); color: var(--pink); }

	.chip {
		position: absolute;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 9px 14px;
		border-radius: 999px;
		background: var(--surface);
		border: 1px solid var(--line-2);
		box-shadow: var(--shadow);
		font-size: 0.86rem;
		font-weight: 700;
		color: var(--ink-2);
		animation: float 6s ease-in-out infinite;
	}
	.chip b { color: var(--ink); font-weight: 900; }
	.c1 { left: -14px; top: 22%; }
	.c2 { left: 6%; bottom: -24px; animation-delay: -3s; }
	.ai { background: var(--blue); color: var(--on-blue); font-size: 0.68rem; font-weight: 900; border-radius: 6px; padding: 1px 6px; }
	@keyframes float { 50% { transform: translateY(-8px); } }

	.caption { margin-top: 64px; text-align: center; color: var(--ink-3); font-size: 0.9rem; max-width: 40rem; margin-inline: auto; }

	@media (max-width: 720px) {
		.stage { margin-top: 40px; }
		.ticker { position: relative; right: auto; bottom: auto; width: 100%; margin-top: 16px; }
		.chip { display: none; }
		.caption { margin-top: 24px; }
	}
</style>
