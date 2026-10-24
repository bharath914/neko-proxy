<script>
	import { onMount } from 'svelte';
	import { repo } from '#lib/site.js';

	// From docs/bench.md. `used` is the share of the budget spent (lower is better);
	// throughput has a goal to beat instead.
	const stats = [
		{ label: 'Core cold start', value: 18, unit: 'ms', goal: 'goal < 50 ms', used: 18 / 50 },
		{ label: 'Proxy overhead, p50', value: 0.035, unit: 'ms', digits: 3, goal: 'goal < 2 ms', used: 0.035 / 2 },
		{ label: 'Sustained throughput', value: 60592, unit: 'req/s', goal: 'goal 5,000 req/s', times: 12 },
		{ label: 'Search, 100k flows', value: 64, unit: 'ms', goal: 'slowest query · goal < 100 ms', used: 64 / 100 },
		{ label: 'App first frame', value: 189, unit: 'ms', goal: 'goal < 1 s', used: 189 / 1000 },
		{ label: 'Core idle memory', value: 12.8, unit: 'MB', digits: 1, goal: 'goal < 40 MB', used: 12.8 / 40 }
	];

	let el;
	let t = $state(0);
	onMount(() => {
		if (matchMedia('(prefers-reduced-motion: reduce)').matches) return void (t = 1);
		const io = new IntersectionObserver(([e]) => {
			if (!e.isIntersecting) return;
			io.disconnect();
			const start = performance.now();
			const step = (now) => {
				const k = Math.min(1, (now - start) / 1400);
				t = 1 - Math.pow(1 - k, 3);
				if (k < 1) requestAnimationFrame(step);
			};
			requestAnimationFrame(step);
		}, { threshold: 0.3 });
		io.observe(el);
		return () => io.disconnect();
	});

	const fmt = (s, k) => (s.value * k).toLocaleString('en-US', { minimumFractionDigits: s.digits ?? 0, maximumFractionDigits: s.digits ?? 0 });
</script>

<section class="block numbers">
	<div class="wrap">
		<div class="head">
			<div>
				<span class="eyebrow">Measured, not promised</span>
				<h2 class="section-title">Fast is a feature. Here are the receipts.</h2>
			</div>
			<p class="section-lede">Every goal was set before the first line of code and benchmarked from the first milestone. Release builds on a 12-core Linux desktop.</p>
		</div>

		<div class="grid" bind:this={el}>
			{#each stats as s}
				<article class="stat card">
					<h3>{s.label}</h3>
					<p class="val"><span class="num">{fmt(s, t)}</span><span class="unit">{s.unit}</span></p>
					{#if s.times}
						<div class="meter beat" aria-hidden="true"><span style="width:{100 * t}%"></span></div>
						<p class="goal"><b>{s.times}×</b> the {s.goal.replace('goal ', 'goal of ')}</p>
					{:else}
						<div class="meter" aria-hidden="true"><span style="width:{Math.max(1.5, s.used * 100) * t}%"></span></div>
						<p class="goal"><b>{Math.max(1, Math.round(s.used * 100))}%</b> of the budget · {s.goal}</p>
					{/if}
				</article>
			{/each}
		</div>
		<p class="note">
			Overhead and throughput are plain HTTP with the origin on the same machine, so treat them as lower bounds. Full method and caveats in
			<a href={`${repo}/blob/main/docs/bench.md`}>docs/bench.md</a>.
		</p>
	</div>
</section>

<style>
	.head { display: grid; grid-template-columns: 1.2fr 1fr; gap: 40px; align-items: end; }
	@media (max-width: 860px) { .head { grid-template-columns: 1fr; gap: 0; } }
	.head .section-lede { margin-bottom: 20px; }
	.grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-top: 40px; }
	@media (max-width: 860px) { .grid { grid-template-columns: 1fr 1fr; } }
	@media (max-width: 520px) { .grid { grid-template-columns: 1fr; } }
	.stat { padding: 24px 24px 22px; }
	h3 { font-size: 0.82rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--ink-3); font-weight: 800; }
	.val { display: flex; align-items: baseline; gap: 8px; margin: 10px 0 16px; }
	.num { font-size: clamp(2.4rem, 5vw, 3.1rem); font-weight: 900; letter-spacing: -0.03em; font-variant-numeric: tabular-nums; line-height: 1; }
	.unit { font-weight: 800; color: var(--ink-2); font-size: 1.05rem; }
	.meter { height: 8px; border-radius: 8px; background: var(--surface-2); border: 1px solid var(--line); overflow: hidden; }
	.meter span { display: block; height: 100%; border-radius: 8px; background: var(--blue); }
	.meter.beat span { background: linear-gradient(90deg, var(--blue), var(--pink)); }
	.goal { margin-top: 10px; font-size: 0.86rem; color: var(--ink-2); }
	.goal b { color: var(--ink); }
	.note { margin-top: 22px; color: var(--ink-3); font-size: 0.88rem; }
</style>
