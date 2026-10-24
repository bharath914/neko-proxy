<script>
	import { onMount } from 'svelte';
	import { flip } from 'svelte/animate';
	import { fly } from 'svelte/transition';
	import { asset } from '$app/paths';
	import { bestDownload } from '#lib/release.js';
	import { reducedMotion } from '#lib/spring.js';
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
		['POST', 'dummyjson.com', '/auth/login', 0, 'held', 'paused'],
		['GET', 'api.open-meteo.com', '/v1/forecast?latitude=12.97…', 200, '728 ms'],
		['DELETE', 'dummyjson.com', '/products/1', 200, '311 ms'],
		['GET', 'registry.npmjs.org', '/svelte/latest', 200, '307 ms']
	];
	let next = 4;
	let rows = $state(feed.slice(0, 4).map((r, i) => ({ id: i, r })));
	const tone = (s) => (s === 0 ? 'held' : s >= 500 ? 'bad' : s >= 400 ? 'warn' : 'ok');

	// The download button names the visitor's platform once we know it.
	let dl = $state({ href: '#download', label: 'Download', note: 'Free and open source · macOS 13+ and Linux' });

	let stage;
	let tilt = $state(1);

	onMount(() => {
		const still = reducedMotion();

		// The screenshot lies back a little and comes upright as you scroll to it:
		// tied 1:1 to the scroll position, so it moves only when you do.
		let raf = 0;
		const measure = () => {
			raf = 0;
			const top = stage.getBoundingClientRect().top;
			tilt = Math.min(1, Math.max(0, (innerHeight - top) / (innerHeight * 0.85)));
		};
		const onScroll = () => (raf ||= requestAnimationFrame(measure));
		if (!still) {
			measure();
			addEventListener('scroll', onScroll, { passive: true });
			addEventListener('resize', onScroll);
		}

		// New rows arrive only while the ticker is on screen.
		let timer = 0;
		const io = new IntersectionObserver(([e]) => {
			clearInterval(timer);
			if (e.isIntersecting && !still) {
				timer = setInterval(() => {
					rows = [{ id: next, r: feed[next % feed.length] }, ...rows].slice(0, 4);
					next++;
				}, 1800);
			}
		});
		io.observe(stage);

		bestDownload().then((d) => {
			if (d) dl = { href: d.file.url, label: d.label, note: d.note };
		});

		return () => {
			removeEventListener('scroll', onScroll);
			removeEventListener('resize', onScroll);
			clearInterval(timer);
			io.disconnect();
		};
	});
</script>

<section class="hero">
	<div class="wrap intro">
		<img class="icon" src={asset('/assets/neko.png')} alt="" width="104" height="104" />
		<p class="name">Neko Proxy</p>
		<h1>See every request.<br /><span class="grad">Change any of them.</span></h1>
		<p class="lede">
			A fast, free HTTP and HTTPS debugging proxy. Inspect traffic from your browser, terminal or phone, mock and
			pause requests mid-flight, and let your AI assistant read along.
		</p>
		<div class="cta">
			<a class="btn lg" href={dl.href}>
				<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 19.5h14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" /></svg>
				{dl.label}
			</a>
			<a class="link-arrow" href="#tour">Take the tour</a>
		</div>
		<p class="note">{dl.note}</p>
	</div>

	<div class="wrap">
		<div class="stage" bind:this={stage} style="--t:{tilt}">
			<div class="tilt">
				<Shot name="inspector" eager alt="The Neko Inspector showing 87 captured requests, with a JSON product response open in a tree view" />
			</div>

			<div class="ticker" aria-hidden="true">
				<div class="tk-head"><span class="rec"></span> Recording <span class="tk-port">127.0.0.1:8899</span></div>
				<div class="tk-rows">
					{#each rows as row (row.id)}
						<div class="tk-row" animate:flip={{ duration: 450 }} in:fly={{ y: -12, duration: 450 }}>
							<span class="m m-{row.r[0].toLowerCase()}">{row.r[0]}</span>
							<span class="u"><b>{row.r[1]}</b>{row.r[2]}</span>
							<span class="s {tone(row.r[3])}">{row.r[3] || '⏸'}</span>
							{#if row.r[5]}<span class="tag">{row.r[5]}</span>{:else}<span class="t">{row.r[4]}</span>{/if}
						</div>
					{/each}
				</div>
			</div>
		</div>
		<p class="caption">Real traffic, captured by Neko on a Mac, with one mock, one throttle and one breakpoint switched on.</p>
	</div>
</section>

<style>
	.hero { padding: 72px 0 96px; overflow: hidden; }
	.intro { text-align: center; display: flex; flex-direction: column; align-items: center; }
	.icon { width: 104px; height: 104px; border-radius: 23px; box-shadow: 0 10px 30px color-mix(in srgb, var(--blue) 30%, transparent); }
	.name { margin-top: 18px; font-size: 1.3rem; font-weight: 600; letter-spacing: -0.02em; }
	h1 { margin-top: 6px; }
	.lede { margin-top: 22px; max-width: 40rem; font-size: clamp(1.15rem, 2vw, 1.45rem); line-height: 1.38; letter-spacing: -0.018em; color: var(--ink-2); }
	.cta { display: flex; align-items: center; justify-content: center; gap: 28px; flex-wrap: wrap; margin-top: 34px; }
	.note { margin-top: 14px; font-size: 0.86rem; color: var(--ink-3); letter-spacing: -0.005em; }

	.stage { position: relative; margin-top: 64px; perspective: 1800px; }
	/* --t runs 0 → 1 as the screenshot scrolls into view. */
	.tilt {
		transform-origin: 50% 100%;
		transform: rotateX(calc((1 - var(--t)) * 14deg)) scale(calc(0.93 + var(--t) * 0.07));
		will-change: transform;
	}

	/* A thick material floating over the screenshot. */
	.ticker {
		position: absolute;
		right: -14px;
		bottom: -40px;
		width: min(440px, 60%);
		padding: 12px;
		border-radius: 18px;
		background: var(--material-thick);
		backdrop-filter: blur(30px) saturate(1.8);
		-webkit-backdrop-filter: blur(30px) saturate(1.8);
		box-shadow: 0 0 0 0.5px var(--line-2), inset 0 1px 0 var(--material-edge), var(--shadow-lg);
		font-size: 0.8rem;
	}
	.tk-head { display: flex; align-items: center; gap: 8px; font-weight: 600; padding: 2px 6px 8px; }
	.rec { width: 8px; height: 8px; border-radius: 50%; background: var(--red); }
	.tk-port { margin-left: auto; font: 0.74rem var(--mono); color: var(--ink-3); }
	.tk-rows { min-height: 148px; }
	.tk-row { display: grid; grid-template-columns: 54px 1fr auto 64px; gap: 8px; align-items: center; padding: 7px 8px; border-radius: 10px; font-family: var(--mono); }
	.tk-row:first-child { background: var(--blue-soft); }
	.m { font-weight: 600; font-size: 0.72rem; color: var(--green); }
	.m-post { color: var(--amber); } .m-put { color: var(--blue); } .m-patch { color: var(--purple); } .m-delete { color: var(--red); }
	.u { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--ink-3); font-size: 0.74rem; }
	.u b { color: var(--ink); font-weight: 500; }
	.s { font-weight: 600; font-size: 0.74rem; }
	.s.ok { color: var(--green); } .s.warn, .s.held { color: var(--amber); } .s.bad { color: var(--red); }
	.t { text-align: right; color: var(--ink-3); font-size: 0.72rem; }
	.tag { justify-self: end; font: 600 0.66rem var(--font); letter-spacing: 0; padding: 2px 7px; border-radius: 999px; background: var(--pink-soft); color: var(--pink); }

	.caption { margin: 72px auto 0; text-align: center; color: var(--ink-3); font-size: 0.86rem; max-width: 36rem; }

	@media (max-width: 734px) {
		.hero { padding: 48px 0 64px; }
		.stage { margin-top: 44px; }
		.ticker { position: relative; right: auto; bottom: auto; width: 100%; margin-top: 18px; }
		.caption { margin-top: 24px; }
	}
	@media (prefers-reduced-motion: reduce) { .tilt { transform: none; } }
</style>
