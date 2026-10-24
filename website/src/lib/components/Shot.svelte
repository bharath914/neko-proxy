<script>
	import { asset } from '$app/paths';

	/** name: a screenshot in static/shots, saved as <name>-light.jpg and <name>-dark.jpg. */
	let { name, alt, eager = false, title = 'Neko' } = $props();
	const load = eager ? 'eager' : 'lazy';
</script>

<figure class="window">
	<div class="bar" aria-hidden="true">
		<span class="dot r"></span><span class="dot y"></span><span class="dot g"></span>
		<span class="title">{title}</span>
	</div>
	<div class="view">
		<img class="only-light" src={asset(`/shots/${name}-light.jpg`)} {alt} width="1800" height="945" loading={load} decoding="async" />
		<img class="only-dark" src={asset(`/shots/${name}-dark.jpg`)} {alt} width="1800" height="945" loading={load} decoding="async" />
	</div>
</figure>

<style>
	.window {
		margin: 0;
		border-radius: 14px;
		overflow: hidden;
		background: var(--surface);
		border: 1px solid var(--line-2);
		box-shadow: var(--shadow-lg);
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 7px;
		height: 32px;
		padding: 0 12px;
		background: var(--surface-2);
		border-bottom: 1px solid var(--line);
	}
	.dot { width: 11px; height: 11px; border-radius: 50%; }
	.r { background: #ff5f57; } .y { background: #febc2e; } .g { background: #28c840; }
	.title { flex: 1; text-align: center; margin-right: 50px; font-size: 0.78rem; font-weight: 700; color: var(--ink-3); }
	.view { aspect-ratio: 1800 / 945; background: var(--surface); }
	.view img { width: 100%; height: 100%; object-fit: cover; object-position: top left; }
</style>
