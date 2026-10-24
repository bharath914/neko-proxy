<script>
	import { onMount } from 'svelte';
	import { repo } from '#lib/site.js';
	import { reveal } from '#lib/reveal.js';
	import { reducedMotion } from '#lib/spring.js';

	// From docs/bench.md. `used` is the share of the budget spent (lower is better);
	// throughput has a goal to beat instead.
	const stats = [
		{ label: 'Core cold start', value: 18, unit: 'ms', goal: 'Goal: under 50 ms', used: 18 / 50 },
		{ label: 'Proxy overhead per request', value: 0.035, unit: 'ms', digits: 3, goal: 'p50. Goal: under 2 ms', used: 0.035 / 2 },
		{ label: 'Sustained throughput', value: 60592, unit: 'req/s', goal: '12× the goal of 5,000', beat: true },
		{ label: 'Search across 100k flows', value: 64, unit: 'ms', goal: 'Slowest query. Goal: under 100 ms', used: 64 / 100 },
		{ label: 'App first frame', value: 189, unit: 'ms', goal: 'Goal: under 1 s', used: 189 / 1000 },
		{ label: 'Core memory at idle', value: 12.8, unit: 'MB', digits: 1, goal: 'Goal: under 40 MB', used: 12.8 / 40 }
	];

	let grid;
	let t = $state(0);
	onMount(() => {
		if (reducedMotion()) return void (t = 1);
		const io = new IntersectionObserver(([e]) => {
			if (!e.isIntersecting) return;
			io.disconnect();
			const start = performance.now();
			const step = (now) => {
				const k = Math.min(1, (now - start) / 1300);
				t = 1 - Math.pow(1 - k, 4);
				if (k < 1) requestAnimationFrame(step);
			};
			requestAnimationFrame(step);
		}, { threshold: 0.35 });
		io.observe(grid);
		return () => io.disconnect();
	});

	const fmt = (s) => (s.value * t).toLocaleString('en-US', { minimumFractionDigits: s.digits ?? 0, maximumFractionDigits: s.digits ?? 0 });
</script>

<section class="block alt">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow tint">Measured, not promised</span>
			<h2 class="section-title">Fast is a feature.<br />Here are the receipts.</h2>
			<p class="section-lede">Every goal was set before the first line of code and benchmarked from the first milestone. Release builds on a 12-core Linux desktop.</p>
		</div>

		<div class="grid" bind:this={grid}>
			{#each stats as s}
				<article class="stat">
					<p class="val"><span class="num">{fmt(s)}</span><span class="unit">{s.unit}</span></p>
					<h3>{s.label}</h3>
					<div class="meter" class:beat={s.beat} aria-hidden="true"><span style="transform:scaleX({(s.beat ? 1 : Math.max(0.015, s.used)) * t})"></span></div>
					<p class="goal">{s.goal}</p>
				</article>
			{/each}
		</div>
		<p class="note">
			Overhead and throughput are plain HTTP with the origin on the same machine, so treat them as lower bounds. Method and caveats:
			<a href={`${repo}/blob/main/docs/bench.md`}>docs/bench.md</a>.
		</p>
	</div>
</section>

<style>
	.grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 48px 40px; margin-top: 72px; }
	@media (max-width: 860px) { .grid { grid-template-columns: 1fr 1fr; } }
	@media (max-width: 520px) { .grid { grid-template-columns: 1fr; gap: 36px; } }
	.stat { border-top: 1px solid var(--line-2); padding-top: 22px; }
	.val { display: flex; align-items: baseline; gap: 6px; }
	.num { font-family: var(--font-display); font-size: clamp(2.8rem, 6vw, 4rem); font-weight: 700; letter-spacing: -0.04em; line-height: 1; font-variant-numeric: tabular-nums; }
	.unit { font-size: 1.15rem; font-weight: 600; color: var(--ink-2); letter-spacing: -0.01em; }
	h3 { font-size: 1.05rem; margin-top: 12px; }
	.meter { height: 6px; margin-top: 16px; border-radius: 6px; background: var(--fill); overflow: hidden; }
	.meter span { display: block; height: 100%; border-radius: 6px; background: var(--blue); transform-origin: 0 50%; }
	.meter.beat span { background: linear-gradient(90deg, var(--blue), #7a5cff, var(--pink)); }
	.goal { margin-top: 10px; font-size: 0.88rem; color: var(--ink-2); }
	.note { margin-top: 56px; text-align: center; color: var(--ink-3); font-size: 0.86rem; }
</style>
