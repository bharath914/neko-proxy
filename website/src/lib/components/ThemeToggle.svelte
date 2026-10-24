<script>
	import { onMount } from 'svelte';

	// 'auto' follows the system; the choice is remembered per browser.
	let mode = $state('auto');
	const order = ['auto', 'light', 'dark'];
	const labels = { auto: 'Appearance: automatic', light: 'Appearance: light', dark: 'Appearance: dark' };

	onMount(() => {
		const t = document.documentElement.dataset.theme;
		if (t === 'light' || t === 'dark') mode = t;
	});

	function cycle() {
		mode = order[(order.indexOf(mode) + 1) % order.length];
		const root = document.documentElement;
		// Cross-fade colors instead of flashing between light and dark.
		root.classList.add('theme-anim');
		setTimeout(() => root.classList.remove('theme-anim'), 450);
		if (mode === 'auto') delete root.dataset.theme;
		else root.dataset.theme = mode;
		try {
			if (mode === 'auto') localStorage.removeItem('neko-theme');
			else localStorage.setItem('neko-theme', mode);
		} catch (e) {}
	}
</script>

<button type="button" onclick={cycle} aria-label={labels[mode]} title={labels[mode]}>
	{#if mode === 'light'}
		<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4.2" /><path d="M12 2.5v2.2M12 19.3v2.2M4.6 4.6l1.6 1.6M17.8 17.8l1.6 1.6M2.5 12h2.2M19.3 12h2.2M4.6 19.4l1.6-1.6M17.8 6.2l1.6-1.6" /></svg>
	{:else if mode === 'dark'}
		<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z" /></svg>
	{:else}
		<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5" /><path d="M12 3.5v17a8.5 8.5 0 0 0 0-17Z" fill="currentColor" /></svg>
	{/if}
</button>

<style>
	button {
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: 999px;
		border: 0;
		background: transparent;
		color: var(--ink);
		opacity: 0.8;
		cursor: pointer;
		transition: transform 0.25s cubic-bezier(0.2, 0.9, 0.3, 1), opacity 0.2s;
	}
	button:hover { opacity: 1; background: var(--fill); }
	button:active { transform: scale(0.88); transition-duration: 0.1s; }
	svg { width: 17px; height: 17px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
</style>
