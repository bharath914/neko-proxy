<script>
	import { onMount } from 'svelte';
	import { asset, resolve } from '$app/paths';
	import { page } from '$app/state';
	import { repo } from '#lib/site.js';
	import { bestDownload } from '#lib/release.js';
	import ThemeToggle from './ThemeToggle.svelte';

	let scrolled = $state(false);
	let home = $derived(page.route.id === '/');
	const link = (hash) => (home ? hash : resolve('/') + hash);

	// Start the right file for this visitor straight away; otherwise show the options.
	let file = $state(null);
	onMount(() => {
		bestDownload().then((d) => (file = d?.file ?? null));
	});
</script>

<svelte:window onscroll={() => (scrolled = window.scrollY > 4)} />

<header class:scrolled>
	<nav class="wrap" aria-label="Main">
		<a class="brand" href={resolve('/')}>
			<img src={asset('/assets/neko.png')} alt="" width="26" height="26" />
			<span>Neko Proxy</span>
		</a>
		<div class="links">
			<a href={link('#why')}>Why</a>
			<a href={link('#tour')}>Tour</a>
			<a href={link('#how')}>How it works</a>
			<a href={link('#ai')}>AI</a>
			<a href={link('#phones')}>Phones</a>
			<a href={resolve('/install-cert')}>Certificate</a>
		</div>
		<span class="sp"></span>
		<a class="gh" href={repo} aria-label="Source on GitHub">
			<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
		</a>
		<ThemeToggle />
		<a class="dl" href={file?.url ?? link('#download')} title={file?.name}>Download</a>
	</nav>
</header>

<style>
	/* A translucent layer the page scrolls under. */
	header {
		position: sticky;
		top: 0;
		z-index: 50;
		background: var(--material);
		backdrop-filter: saturate(1.8) blur(20px);
		-webkit-backdrop-filter: saturate(1.8) blur(20px);
	}
	/* Scroll edge: a soft shadow that appears only once content slides underneath. */
	header::after {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		top: 100%;
		height: 14px;
		background: linear-gradient(to bottom, color-mix(in srgb, var(--ink) 7%, transparent), transparent);
		opacity: 0;
		transition: opacity 0.3s ease;
		pointer-events: none;
	}
	header.scrolled::after { opacity: 1; }
	nav { display: flex; align-items: center; gap: 6px; height: 52px; }
	.brand { display: flex; align-items: center; gap: 9px; color: var(--ink); font-weight: 600; font-size: 1.05rem; letter-spacing: -0.02em; margin-right: 14px; text-decoration: none !important; }
	.brand img { border-radius: 7px; }
	.links { display: flex; gap: 2px; }
	.links a { color: var(--ink); opacity: 0.8; font-size: 0.82rem; letter-spacing: -0.005em; padding: 6px 10px; border-radius: 8px; transition: opacity 0.2s; text-decoration: none !important; }
	.links a:hover { opacity: 1; }
	.links a:active { opacity: 0.55; transition: none; }
	.sp { flex: 1; }
	.gh { display: grid; place-items: center; width: 36px; height: 36px; border-radius: 999px; color: var(--ink); opacity: 0.8; }
	.gh:hover { opacity: 1; background: var(--fill); }
	.gh svg { width: 17px; height: 17px; }
	.dl {
		margin-left: 6px;
		font-size: 0.82rem;
		font-weight: 500;
		padding: 5px 14px;
		border-radius: 999px;
		background: var(--blue);
		color: var(--on-blue);
		text-decoration: none !important;
		transition: transform 0.25s cubic-bezier(0.2, 0.9, 0.3, 1);
	}
	.dl:active { transform: scale(0.94); transition-duration: 0.1s; }
	@media (max-width: 920px) { .links { display: none; } }
	@media (max-width: 380px) { .brand span { display: none; } }
</style>
