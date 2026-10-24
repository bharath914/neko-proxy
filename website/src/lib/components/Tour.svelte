<script>
	import { onMount } from 'svelte';
	import { fade } from 'svelte/transition';
	import { reveal } from '#lib/reveal.js';
	import { createSpring, project, rubberband, reducedMotion } from '#lib/spring.js';
	import Shot from './Shot.svelte';

	const tabs = [
		{
			id: 'inspector',
			name: 'Inspector',
			title: 'Every request, the moment it happens.',
			text: 'Hosts on the left, flows in the middle, the full request and response below. Bodies are decoded for you: gzip, brotli and zstd unpacked, JSON folded into a tree, images previewed.',
			points: ['Search 100,000 flows in milliseconds', 'Errors, Slow and Pinned filters', 'Copy as cURL, replay, mock or break from any row'],
			alt: 'Inspector with a dummyjson product search response shown as a JSON tree'
		},
		{
			id: 'timing',
			name: 'Timing',
			title: 'See where the milliseconds went.',
			text: 'Each flow records DNS, connect, TLS, send, wait and receive. The waterfall shows at a glance whether a slow call is the network, the handshake or the server thinking.',
			points: ['New vs reused connections', 'App time vs wire time for SDK flows', 'Slow calls flagged in the table'],
			alt: 'Timing waterfall for a request to pokeapi.co: DNS 6 ms, connect 10 ms, TLS 12 ms, wait 23 ms'
		},
		{
			id: 'rules',
			name: 'Rules',
			title: 'Bend traffic without touching the server.',
			text: 'Mock a response, map a host to staging, rewrite headers, add latency or pause a request. Rules apply in order, switch on and off with one toggle, and are saved as plain JSON.',
			points: ['Mock · Map remote · Map local · Rewrite · Throttle · Breakpoint', 'Import and export as a bundle', 'Changes apply to the next request, no restart'],
			alt: 'Rules screen with five rules: Checkout is down, Slow product images, Skip GitHub cache, Staging API and Hold login'
		},
		{
			id: 'breakpoints',
			name: 'Breakpoints',
			title: 'Pause a request. Edit it. Let it go.',
			text: 'A breakpoint holds the request or the response right where it is. Change the method, path, headers or body, then resume or abort. Here a login is held while an AI client waits for approval to add a mock.',
			points: ['Request, response or both', 'Times out and lets traffic through, so nothing hangs forever', 'AI changes wait for your Allow'],
			alt: 'Breakpoints screen holding POST dummyjson.com/auth/login, with an AI client approval card in the corner'
		}
	];
	const n = tabs.length;
	const GAP = 40;

	let current = $state(0);
	let viewport, track, thumb;
	let slides = [];
	let buttons = [];
	let W = 1;
	let spring;

	// Everything visual derives from one number: the track's x offset.
	function paint(x) {
		track.style.transform = `translate3d(${x}px,0,0)`;
		const f = -x / W;
		for (let i = 0; i < n; i++) {
			const d = Math.min(1, Math.abs(i - f));
			slides[i].style.opacity = String(1 - d * 0.55);
			slides[i].style.transform = `scale(${1 - d * 0.04})`;
		}
		// The segment highlight slides between tabs as the gallery moves.
		const c = Math.min(n - 1, Math.max(0, f));
		const a = buttons[Math.floor(c)];
		const b = buttons[Math.ceil(c)];
		const t = c - Math.floor(c);
		thumb.style.transform = `translateX(${a.offsetLeft + (b.offsetLeft - a.offsetLeft) * t}px)`;
		thumb.style.width = `${a.offsetWidth + (b.offsetWidth - a.offsetWidth) * t}px`;
		const r = Math.round(c);
		if (r !== current) current = r;
	}

	function go(i, opts = {}) {
		i = Math.max(0, Math.min(n - 1, i));
		if (reducedMotion()) return spring.set(-i * W);
		spring.to(-i * W, opts);
	}

	// Direct manipulation: follow the pointer 1:1 once it clearly moves sideways.
	let drag = null;
	function down(e) {
		if (e.button !== 0) return;
		// Catch it on touch-down, wherever it is right now, even mid-flight.
		spring.stop();
		drag = { id: e.pointerId, x0: e.clientX, y0: e.clientY, axis: null, hist: [] };
	}
	const settle = () => go(Math.round(-spring.value / W));
	function move(e) {
		if (!drag || e.pointerId !== drag.id) return;
		if (!drag.axis) {
			const dx = e.clientX - drag.x0;
			const dy = e.clientY - drag.y0;
			if (Math.hypot(dx, dy) < 10) return;
			if (Math.abs(dy) > Math.abs(dx)) {
				drag = null; // a scroll, not a swipe
				return settle();
			}
			drag.axis = 'x';
			drag.x0 = e.clientX;
			drag.start = spring.value;
			viewport.setPointerCapture(e.pointerId);
		}
		let x = drag.start + (e.clientX - drag.x0);
		const min = -(n - 1) * W;
		if (x > 0) x = rubberband(x, W);
		else if (x < min) x = min + rubberband(x - min, W);
		spring.set(x);
		drag.hist.push([e.timeStamp, e.clientX]);
		while (drag.hist.length > 2 && e.timeStamp - drag.hist[0][0] > 100) drag.hist.shift();
	}
	function up(e) {
		if (!drag || e.pointerId !== drag.id) return;
		const d = drag;
		drag = null;
		if (d.axis !== 'x') return settle();
		const [t0, x0] = d.hist[0] ?? [0, 0];
		const [t1, x1] = d.hist.at(-1) ?? [0, 0];
		const v = t1 > t0 ? ((x1 - x0) / (t1 - t0)) * 1000 : 0;
		// Aim for where the flick would land, one page at most from where it began,
		// and carry the finger's speed into the spring so there's no seam.
		const from = Math.round(-d.start / W);
		const aim = Math.round(-(spring.value + project(v)) / W);
		go(Math.max(from - 1, Math.min(from + 1, aim)), { velocity: v, damping: 0.86, response: 0.42 });
	}

	function key(e) {
		const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
		if (!step) return;
		e.preventDefault();
		const i = Math.max(0, Math.min(n - 1, current + step));
		go(i);
		buttons[i]?.focus();
	}

	onMount(() => {
		W = viewport.clientWidth + GAP;
		spring = createSpring(0, { response: 0.45, damping: 1, epsilon: 0.3, onUpdate: paint });
		paint(0);
		const ro = new ResizeObserver(() => {
			W = viewport.clientWidth + GAP;
			spring.set(-current * W);
		});
		ro.observe(viewport);
		return () => {
			ro.disconnect();
			spring.stop();
		};
	});
</script>

<section class="block alt tour" id="tour">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow tint">A quick tour</span>
			<h2 class="section-title">Everything a proxy should do.<br />None of the hairballs.</h2>
			<p class="section-lede">Real screenshots of the Neko app, recorded while it captured live traffic. Drag, swipe or pick a screen.</p>
		</div>

		<div class="seg-wrap">
			<div class="seg" role="tablist" aria-label="Screens" tabindex="-1" onkeydown={key}>
				<span class="thumb" bind:this={thumb} aria-hidden="true"></span>
				{#each tabs as t, i}
					<button
						bind:this={buttons[i]}
						type="button"
						role="tab"
						aria-selected={i === current}
						aria-controls="tour-gallery"
						tabindex={i === current ? 0 : -1}
						onclick={() => go(i)}
					>{t.name}</button>
				{/each}
			</div>
		</div>

		<div
			class="viewport"
			id="tour-gallery"
			bind:this={viewport}
			onpointerdown={down}
			onpointermove={move}
			onpointerup={up}
			onpointercancel={up}
			role="group"
			aria-roledescription="carousel"
			aria-label="Screenshots"
		>
			<div class="track" bind:this={track} style="gap:{GAP}px">
				{#each tabs as t, i}
					<div class="slide" bind:this={slides[i]} aria-hidden={i !== current}>
						<Shot name={t.id} alt={t.alt} eager={i < 2} />
					</div>
				{/each}
			</div>
		</div>

		<div class="text">
			{#key current}
				<div class="pane" in:fade={{ duration: 260 }}>
					<div>
						<h3>{tabs[current].title}</h3>
						<p>{tabs[current].text}</p>
					</div>
					<ul>
						{#each tabs[current].points as p}<li>{p}</li>{/each}
					</ul>
				</div>
			{/key}
		</div>
	</div>
</section>

<style>
	.tour { overflow: hidden; }
	.seg-wrap { display: flex; justify-content: center; margin: 48px 0 36px; }
	.seg { position: relative; display: inline-flex; padding: 3px; border-radius: 999px; background: var(--fill); max-width: 100%; overflow-x: auto; scrollbar-width: none; }
	.thumb { position: absolute; top: 3px; bottom: 3px; left: 0; border-radius: 999px; background: var(--surface); box-shadow: 0 1px 3px rgb(0 0 0 / 0.12), 0 0 0 0.5px var(--line); }
	.seg button {
		position: relative;
		flex: none;
		font: 500 0.92rem var(--font);
		letter-spacing: -0.01em;
		color: var(--ink-2);
		background: none;
		border: 0;
		padding: 8px 20px;
		border-radius: 999px;
		cursor: pointer;
		transition: color 0.2s;
	}
	.seg button[aria-selected='true'] { color: var(--ink); }
	.seg button:active { opacity: 0.6; }
	@media (max-width: 480px) { .seg button { padding: 7px 13px; font-size: 0.85rem; } }

	/* The neighbours peek in from beyond the column, so it reads as a gallery. */
	.viewport { touch-action: pan-y; cursor: grab; user-select: none; -webkit-user-select: none; }
	.viewport:active { cursor: grabbing; }
	.track { display: flex; will-change: transform; }
	.slide { flex: 0 0 100%; transform-origin: 50% 50%; }

	.text { display: grid; margin-top: 44px; }
	.pane { grid-area: 1 / 1; display: grid; grid-template-columns: 1.25fr 1fr; gap: 16px 56px; max-width: 60rem; margin: 0 auto; width: 100%; }
	.pane > * { min-width: 0; }
	@media (max-width: 820px) { .pane { grid-template-columns: 1fr; } }
	.pane h3 { font-size: 1.6rem; letter-spacing: -0.024em; margin-bottom: 10px; }
	.pane p { color: var(--ink-2); }
	ul { list-style: none; padding: 0; margin: 4px 0 0; display: grid; gap: 12px; }
	li { display: flex; gap: 12px; font-weight: 500; font-size: 0.98rem; }
	li::before { content: ''; flex: none; width: 20px; height: 20px; margin-top: 1px; border-radius: 50%; background: var(--blue) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='3.2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M6 12.5l4 4L18 8'/%3E%3C/svg%3E") center / 12px no-repeat; }
</style>
