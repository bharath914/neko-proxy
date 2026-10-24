<script>
	import { reveal } from '#lib/reveal.js';
	import Shot from './Shot.svelte';

	const tabs = [
		{
			id: 'inspector',
			name: 'Inspector',
			title: 'Every request, the moment it happens',
			text: 'Hosts on the left, flows in the middle, the full request and response below. Bodies are decoded for you: gzip, brotli and zstd unpacked, JSON folded into a tree, images previewed.',
			points: ['Search 100,000 flows in milliseconds', 'Errors, Slow and Pinned filters', 'Copy as cURL, replay, mock or break from any row'],
			alt: 'Inspector with a dummyjson product search response shown as a JSON tree'
		},
		{
			id: 'timing',
			name: 'Timing',
			title: 'Where the milliseconds went',
			text: 'Each flow records DNS, connect, TLS, send, wait and receive. The waterfall shows at a glance whether a slow call is the network, the handshake or the server thinking.',
			points: ['New vs reused connections', 'App time vs wire time for SDK flows', 'Slow calls flagged in the table'],
			alt: 'Timing waterfall for a request to pokeapi.co: DNS 6 ms, connect 10 ms, TLS 12 ms, wait 23 ms'
		},
		{
			id: 'rules',
			name: 'Rules',
			title: 'Bend traffic without touching the server',
			text: 'Mock a response, map a host to staging, rewrite headers, add latency or pause a request. Rules apply in order, switch on and off with one toggle, and are saved as plain JSON.',
			points: ['Mock · Map remote · Map local · Rewrite · Throttle · Breakpoint', 'Import and export as a bundle', 'Changes apply to the next request, no restart'],
			alt: 'Rules screen with five rules: Checkout is down, Slow product images, Skip GitHub cache, Staging API and Hold login'
		},
		{
			id: 'breakpoints',
			name: 'Breakpoints',
			title: 'Pause a request. Edit it. Let it go.',
			text: 'A breakpoint holds the request (or the response) right where it is. Change the method, path, headers or body, then resume or abort. Here a login is held while an AI client waits for approval to add a mock.',
			points: ['Request, response or both', 'Times out and lets traffic through, so nothing hangs forever', 'AI changes wait for your Allow'],
			alt: 'Breakpoints screen holding POST dummyjson.com/auth/login, with an AI client approval card in the corner'
		}
	];
	let active = $state(0);
	const tab = $derived(tabs[active]);

	function key(e) {
		if (e.key === 'ArrowDown' || e.key === 'ArrowRight') active = (active + 1) % tabs.length;
		else if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') active = (active + tabs.length - 1) % tabs.length;
		else return;
		e.preventDefault();
		document.getElementById(`tour-tab-${active}`)?.focus();
	}
</script>

<section class="block" id="tour">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow">A quick tour</span>
			<h2 class="section-title">Everything a proxy should do, <span class="accent">none of the hairballs.</span></h2>
			<p class="section-lede">These are real screenshots of the Neko app, recorded while it captured live traffic. Nothing is mocked up, except the one response we mocked on purpose.</p>
		</div>

		<div class="tour" use:reveal>
			<div class="tabs" role="tablist" aria-label="Screens" tabindex="-1" onkeydown={key}>
				{#each tabs as t, i}
					<button
						id="tour-tab-{i}"
						type="button"
						role="tab"
						aria-selected={i === active}
						aria-controls="tour-panel"
						tabindex={i === active ? 0 : -1}
						onclick={() => (active = i)}
					>
						<span class="n">0{i + 1}</span>
						<span class="name">{t.name}</span>
					</button>
				{/each}
			</div>

			<div class="panel" id="tour-panel" role="tabpanel" aria-labelledby="tour-tab-{active}">
				<div class="text">
					<div>
						<h3>{tab.title}</h3>
						<p>{tab.text}</p>
					</div>
					<ul>
						{#each tab.points as p}<li>{p}</li>{/each}
					</ul>
				</div>
				<div class="frames">
					{#each tabs as t, i}
						<div class="frame" class:on={i === active} aria-hidden={i !== active}>
							<Shot name={t.id} alt={t.alt} />
						</div>
					{/each}
				</div>
			</div>
		</div>
	</div>
</section>

<style>
	.tour { margin-top: 48px; }
	.tabs { display: flex; justify-content: center; gap: 6px; padding: 6px; margin: 0 auto 28px; width: fit-content; max-width: 100%; overflow-x: auto; background: var(--surface); border: 1px solid var(--line); border-radius: 999px; box-shadow: var(--shadow-sm); }
	.tabs button { display: inline-flex; align-items: center; gap: 8px; flex: none; font: inherit; font-weight: 800; font-size: 0.95rem; color: var(--ink-2); background: transparent; border: 0; border-radius: 999px; padding: 9px 18px; cursor: pointer; transition: background 0.25s, color 0.25s; }
	.tabs button:hover { color: var(--ink); }
	.tabs button[aria-selected='true'] { background: var(--blue); color: var(--on-blue); }
	.n { font: 600 0.72rem var(--mono); opacity: 0.6; }
	@media (max-width: 560px) { .n { display: none; } .tabs button { padding: 8px 13px; font-size: 0.88rem; } }

	.panel { display: grid; gap: 28px; }
	.text { display: grid; grid-template-columns: 1.2fr 1fr; gap: 16px 48px; align-items: start; max-width: 62rem; margin: 0 auto; width: 100%; }
	.text > div { min-width: 0; }
	@media (max-width: 860px) { .text { grid-template-columns: 1fr; } }
	.text h3 { font-size: 1.6rem; margin-bottom: 10px; }
	.text p { color: var(--ink-2); }
	ul { list-style: none; padding: 0; margin: 4px 0 0; display: grid; gap: 10px; }
	li { display: flex; gap: 10px; font-weight: 700; font-size: 0.95rem; }
	li::before { content: ''; flex: none; width: 18px; height: 18px; margin-top: 3px; border-radius: 6px; background: var(--blue-soft) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%232f7bff' stroke-width='3' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M5 12l5 5L20 7'/%3E%3C/svg%3E") center / 12px no-repeat; }

	.frames { display: grid; }
	.frame { grid-area: 1 / 1; opacity: 0; transform: translateY(10px) scale(0.985); transition: opacity 0.45s ease, transform 0.6s cubic-bezier(0.2, 0.8, 0.2, 1); pointer-events: none; }
	.frame.on { opacity: 1; transform: none; pointer-events: auto; }
</style>
