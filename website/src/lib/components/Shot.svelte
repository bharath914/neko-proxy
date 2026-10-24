<script>
	import { asset } from '$app/paths';

	/** name: a screenshot in static/shots, saved as <name>-light.jpg and <name>-dark.jpg. */
	let { name, alt, eager = false } = $props();
	const load = eager ? 'eager' : 'lazy';
</script>

<figure class="window">
	<div class="bar" aria-hidden="true"><span></span><span></span><span></span></div>
	<div class="view">
		<img class="only-light" src={asset(`/shots/${name}-light.jpg`)} {alt} width="1800" height="945" loading={load} decoding="async" draggable="false" />
		<img class="only-dark" src={asset(`/shots/${name}-dark.jpg`)} {alt} width="1800" height="945" loading={load} decoding="async" draggable="false" />
	</div>
</figure>

<style>
	.window {
		margin: 0;
		border-radius: 12px;
		overflow: hidden;
		background: var(--surface);
		box-shadow: 0 0 0 0.5px var(--line-2), var(--shadow-lg);
	}
	.bar { display: flex; gap: 7px; align-items: center; height: 28px; padding: 0 12px; background: var(--surface-2); }
	.bar span { width: 11px; height: 11px; border-radius: 50%; background: #ff5f57; }
	.bar span:nth-child(2) { background: #febc2e; }
	.bar span:nth-child(3) { background: #28c840; }
	.view { aspect-ratio: 1800 / 945; background: var(--surface); }
	.view img { width: 100%; height: 100%; object-fit: cover; object-position: top left; user-select: none; -webkit-user-drag: none; }
</style>
