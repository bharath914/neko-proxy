<script>
	import { onMount } from 'svelte';
	import { fade } from 'svelte/transition';
	import { reveal } from '#lib/reveal.js';
	import { createSpring, reducedMotion } from '#lib/spring.js';

	const stages = [
		['Accept', 'A connection arrives on 127.0.0.1:8899: plain HTTP, CONNECT or SOCKS5.'],
		['Intercept TLS', 'A leaf certificate is minted from your own CA. Apps that pin their certificate are tunneled untouched and flagged.'],
		['Parse', 'HTTP/1.1, HTTP/2 or WebSocket, with bounded buffers and fuzzed parsers.'],
		['Request rules', 'Breakpoint, map, mock or rewrite, in the order you listed them.'],
		['Forward', 'Sent upstream through any throttle, timing DNS, connect, TLS and wait.'],
		['Response rules', 'Rewrite or hold the response before your app sees it.'],
		['Persist', 'SQLite metadata and zstd-compressed bodies, written off the hot path.'],
		['Publish', 'A small delta goes to every subscriber: the app, the CLI, your AI client.']
	];

	const crates = [
		['net', 'listeners, HTTP/1.1, HTTP/2, WebSocket'],
		['tls', 'CA, leaf cache, pinning detection'],
		['flow', 'flow model and pipeline'],
		['rules', 'mock, map, rewrite, throttle, break'],
		['store', 'SQLite WAL, zstd blobs, FTS'],
		['decode', 'gzip, br, zstd, JSON, images'],
		['api', 'WebSocket + protobuf'],
		['mcp', 'MCP server and redaction'],
		['cli', 'headless runs, HAR export']
	];

	let active = $state(0);
	let reached = $state(0);
	let rail, line, fill, packet;
	let nodes = [];
	let pts = [];
	let sx, sy;
	let timer = 0;
	let ready = $state(false);

	// Node centres, relative to the rail. Re-measured on resize, since the layout
	// turns vertical on narrow screens.
	function measure() {
		const r = rail.getBoundingClientRect();
		pts = nodes.map((el) => {
			const b = el.getBoundingClientRect();
			return { x: b.left + b.width / 2 - r.left, y: b.top + b.height / 2 - r.top };
		});
		const a = pts[0];
		const z = pts.at(-1);
		line.style.left = fill.style.left = `${a.x}px`;
		line.style.top = fill.style.top = `${a.y}px`;
		line.style.width = fill.style.width = `${Math.max(3, z.x - a.x)}px`;
		line.style.height = fill.style.height = `${Math.max(3, z.y - a.y)}px`;
	}

	// X and Y are separate springs, so a diagonal move stays in sync.
	function paint() {
		if (!sx || !sy || !pts.length) return;
		const x = sx.value;
		const y = sy.value;
		packet.style.transform = `translate(${x}px, ${y}px)`;
		const a = pts[0];
		const z = pts.at(-1);
		const horiz = z.x - a.x >= z.y - a.y;
		const p = horiz ? (x - a.x) / (z.x - a.x || 1) : (y - a.y) / (z.y - a.y || 1);
		fill.style.transform = horiz ? `scaleX(${Math.max(0, p)})` : `scaleY(${Math.max(0, p)})`;
		const near = Math.round(Math.max(0, Math.min(1, p)) * (stages.length - 1));
		if (near !== reached) reached = near;
	}

	function go(i, user = true) {
		if (user) clearInterval(timer);
		active = i;
		const { x, y } = pts[i];
		if (reducedMotion()) {
			sx.set(x);
			sy.set(y);
			return;
		}
		sx.to(x);
		sy.to(y);
	}

	function play() {
		clearInterval(timer);
		go(0, false);
		let i = 0;
		timer = setInterval(() => {
			if (++i >= stages.length) return clearInterval(timer);
			go(i, false);
		}, 900);
	}

	onMount(() => {
		measure();
		const opts = { response: 0.5, damping: 1, epsilon: 0.2, onUpdate: paint };
		sx = createSpring(pts[0].x, opts);
		sy = createSpring(pts[0].y, opts);
		paint();
		ready = true;
		const ro = new ResizeObserver(() => {
			measure();
			sx.set(pts[active].x);
			sy.set(pts[active].y);
		});
		ro.observe(rail);
		// Play through once, the first time it scrolls into view.
		const io = new IntersectionObserver(
			([e]) => {
				if (!e.isIntersecting) return;
				io.disconnect();
				if (!reducedMotion()) play();
			},
			{ threshold: 0.5 }
		);
		io.observe(rail);
		return () => {
			ro.disconnect();
			io.disconnect();
			clearInterval(timer);
			sx.stop();
			sy.stop();
		};
	});
</script>

<section class="block" id="how">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow tint">Under the hood</span>
			<h2 class="section-title">The life of a request,<br />in eight steps.</h2>
			<p class="section-lede">The data path never waits on the UI or the disk. Your app gets its response first, and Neko writes things down afterwards. Tap a step to follow a request through.</p>
		</div>

		<div class="rail" class:ready bind:this={rail} use:reveal>
			<span class="line" bind:this={line} aria-hidden="true"></span>
			<span class="fill" bind:this={fill} aria-hidden="true"></span>
			<span class="packet" bind:this={packet} aria-hidden="true"></span>
			<ol>
				{#each stages as [name], i}
					<li>
						<button
							type="button"
							bind:this={nodes[i]}
							class:lit={i <= reached}
							class:on={i === active}
							aria-pressed={i === active}
							onclick={() => go(i)}
						>{i + 1}</button>
						<span class="name">{name}</span>
					</li>
				{/each}
			</ol>
		</div>

		<div class="detail">
			{#key active}
				<div class="d-in" in:fade={{ duration: 220 }}>
					<span class="step">Step {active + 1} of {stages.length}</span>
					<h3>{stages[active][0]}</h3>
					<p>{stages[active][1]}</p>
				</div>
			{/key}
			<div class="d-btns">
				<button type="button" class="btn secondary" onclick={play}>
					<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5.5v13l11-6.5z" fill="currentColor" /></svg>
					Send a request
				</button>
			</div>
		</div>

		<div class="arch" use:reveal>
			<h3>How the pieces fit</h3>
			<div class="layer clients">
				<span class="lbl">Clients</span>
				<div class="row">
					<span class="box">Desktop app<small>Rust and egui on Linux, AppKit on macOS</small></span>
					<span class="box">In-app SDK<small>Android, iOS in progress</small></span>
					<span class="box">AI clients<small>MCP over loopback HTTP</small></span>
					<span class="box">CLI<small>nekod flows, export, ca</small></span>
				</div>
			</div>
			<div class="pipe"><span>WebSocket + protobuf · token on every connection</span></div>
			<div class="layer core">
				<span class="lbl"><b>nekod</b>, the Rust core</span>
				<div class="crates">
					{#each crates as [c, d]}
						<span class="crate"><code>neko-{c}</code><small>{d}</small></span>
					{/each}
				</div>
			</div>
			<div class="pipe"><span>HTTP/1.1 · HTTP/2 · WebSocket</span></div>
			<div class="layer net"><span class="lbl">Your apps, browsers and phones ⇄ the internet</span></div>
		</div>
	</div>
</section>

<style>
	.rail { position: relative; margin-top: 64px; }
	.line, .fill { position: absolute; border-radius: 3px; transform-origin: 0 0; }
	.line { background: var(--fill-2); }
	.fill { background: linear-gradient(90deg, var(--blue), #7a5cff, var(--pink)); transform: scaleX(0); }
	.packet {
		position: absolute;
		left: -9px;
		top: -9px;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		background: var(--surface);
		border: 4px solid var(--blue);
		box-shadow: 0 0 0 6px color-mix(in srgb, var(--blue) 20%, transparent), 0 4px 12px rgb(0 0 0 / 0.2);
		z-index: 2;
		pointer-events: none;
		opacity: 0;
		transition: opacity 0.3s;
	}
	.ready .packet { opacity: 1; }
	ol { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(8, 1fr); gap: 8px; position: relative; }
	li { display: flex; flex-direction: column; align-items: center; gap: 12px; text-align: center; }
	li button {
		position: relative;
		z-index: 1;
		width: 44px;
		height: 44px;
		border-radius: 50%;
		border: 0;
		background: var(--surface-2);
		box-shadow: inset 0 0 0 1.5px var(--line-2);
		font: 600 0.9rem var(--font);
		color: var(--ink-2);
		cursor: pointer;
		transition: transform 0.3s cubic-bezier(0.2, 0.9, 0.3, 1), background-color 0.3s, color 0.3s, box-shadow 0.3s;
	}
	li button.lit { background: var(--blue); color: var(--on-blue); box-shadow: none; }
	li button.on { transform: scale(1.12); }
	li button:active { transform: scale(0.9); transition-duration: 0.1s; }
	.name { font-size: 0.86rem; font-weight: 600; letter-spacing: -0.01em; }

	.detail { display: grid; grid-template-columns: 1fr auto; align-items: end; gap: 24px; max-width: 52rem; margin: 48px auto 0; padding: 30px 32px; border-radius: var(--r-lg); background: var(--bg-alt); }
	.d-in { min-height: 7.2rem; }
	.step { font-size: 0.82rem; font-weight: 600; color: var(--blue); }
	.detail h3 { font-size: 1.6rem; letter-spacing: -0.024em; margin: 6px 0 8px; }
	.detail p { color: var(--ink-2); }

	@media (max-width: 900px) {
		ol { grid-template-columns: 1fr; gap: 14px; }
		li { flex-direction: row; text-align: left; gap: 16px; }
		.detail { grid-template-columns: 1fr; padding: 24px; }
	}

	.arch { margin-top: 96px; }
	.arch h3 { font-size: 1.6rem; letter-spacing: -0.024em; text-align: center; margin-bottom: 28px; }
	.layer { border-radius: var(--r-lg); padding: 22px; background: var(--bg-alt); }
	.lbl { display: block; font-size: 0.82rem; font-weight: 600; color: var(--ink-2); margin-bottom: 14px; }
	.lbl b { color: var(--blue); font-weight: 600; }
	.row { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }
	@media (max-width: 760px) { .row { grid-template-columns: 1fr 1fr; } }
	.box, .crate { display: flex; flex-direction: column; gap: 3px; padding: 14px 16px; border-radius: 14px; background: var(--surface); font-weight: 600; font-size: 0.98rem; letter-spacing: -0.014em; }
	.box small, .crate small { font-weight: 400; color: var(--ink-2); }
	.pipe { position: relative; height: 54px; display: grid; place-items: center; }
	.pipe::before { content: ''; position: absolute; top: 0; bottom: 0; left: 50%; width: 1.5px; background: var(--line-2); }
	.pipe span { position: relative; background: var(--bg); padding: 3px 14px; font: 0.74rem var(--mono); color: var(--ink-2); border-radius: 999px; box-shadow: inset 0 0 0 1px var(--line-2); text-align: center; }
	.core { background: linear-gradient(135deg, color-mix(in srgb, var(--blue) 10%, var(--bg-alt)), color-mix(in srgb, var(--pink) 7%, var(--bg-alt))); }
	.crates { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
	@media (max-width: 760px) { .crates { grid-template-columns: 1fr 1fr; } }
	.crate code { background: none; padding: 0; font-size: 0.88rem; font-weight: 600; color: var(--ink); }
	.net { text-align: center; padding: 16px; }
	.net .lbl { margin: 0; }
</style>
