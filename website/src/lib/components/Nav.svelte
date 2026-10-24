<script>
	import { asset, resolve } from '$app/paths';
	import { page } from '$app/state';
	import { repo } from '#lib/site.js';
	import ThemeToggle from './ThemeToggle.svelte';

	let scrolled = $state(false);
	let home = $derived(page.route.id === '/');
	const link = (hash) => (home ? hash : resolve('/') + hash);
</script>

<svelte:window onscroll={() => (scrolled = window.scrollY > 8)} />

<header class:scrolled>
	<nav class="wrap">
		<a class="brand" href={resolve('/')}>
			<img src={asset('/assets/neko.png')} alt="" width="34" height="34" />
			<span>Neko <b>Proxy</b></span>
		</a>
		<div class="links">
			<a href={link('#why')}>Why</a>
			<a href={link('#tour')}>Tour</a>
			<a href={link('#ai')}>AI</a>
			<a href={link('#roadmap')}>Roadmap</a>
			<a href={resolve('/install-cert')}>Certificate</a>
		</div>
		<span class="sp"></span>
		<ThemeToggle />
		<a class="gh" href={repo} aria-label="Neko Proxy on GitHub">
			<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
			<span>GitHub</span>
		</a>
	</nav>
</header>

<style>
	header {
		position: sticky;
		top: 0;
		z-index: 50;
		transition: background 0.3s, border-color 0.3s, backdrop-filter 0.3s;
		border-bottom: 1px solid transparent;
	}
	header.scrolled {
		background: color-mix(in srgb, var(--bg) 78%, transparent);
		backdrop-filter: saturate(1.6) blur(16px);
		-webkit-backdrop-filter: saturate(1.6) blur(16px);
		border-bottom-color: var(--line);
	}
	nav { display: flex; align-items: center; gap: 22px; height: 64px; }
	.brand { display: flex; align-items: center; gap: 10px; color: var(--ink); text-decoration: none; font-weight: 800; font-size: 1.1rem; letter-spacing: -0.01em; }
	.brand img { width: 34px; height: 34px; border-radius: 9px; box-shadow: var(--shadow-sm); }
	.brand b { font-weight: 800; color: var(--ink-3); }
	.links { display: flex; gap: 4px; }
	.links a, .gh {
		color: var(--ink-2);
		text-decoration: none;
		font-weight: 700;
		font-size: 0.95rem;
		padding: 6px 12px;
		border-radius: 999px;
		transition: background 0.2s, color 0.2s;
	}
	.links a:hover, .gh:hover { color: var(--ink); background: var(--surface-2); }
	.sp { flex: 1; }
	.gh { display: inline-flex; align-items: center; gap: 8px; border: 1px solid var(--line); background: var(--surface); }
	.gh svg { width: 17px; height: 17px; }
	@media (max-width: 860px) { .links { display: none; } }
	@media (max-width: 420px) { .gh span { display: none; } nav { gap: 12px; } }
</style>
