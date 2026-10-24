<script>
	import { onMount } from 'svelte';

	// 'auto' follows the system; the choice is remembered per browser.
	let mode = $state('auto');
	const order = ['auto', 'light', 'dark'];
	const labels = { auto: 'Theme: follow system', light: 'Theme: light', dark: 'Theme: dark' };

	onMount(() => {
		const t = document.documentElement.dataset.theme;
		if (t === 'light' || t === 'dark') mode = t;
	});

	function cycle() {
		mode = order[(order.indexOf(mode) + 1) % order.length];
		const root = document.documentElement;
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
		<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4.5" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></svg>
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
		width: 38px;
		height: 38px;
		border-radius: 999px;
		border: 1px solid var(--line);
		background: var(--surface);
		color: var(--ink-2);
		cursor: pointer;
		transition: color 0.2s, border-color 0.2s, transform 0.2s;
	}
	button:hover { color: var(--ink); border-color: var(--line-2); }
	button:active { transform: scale(0.94); }
	svg { width: 18px; height: 18px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
</style>
