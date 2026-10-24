<script>
	/** A terminal-style block with a copy button. `lines` starting with # are shown as comments. */
	let { code, label = '' } = $props();
	let copied = $state(false);

	async function copy() {
		const text = code
			.split('\n')
			.filter((l) => !l.trimStart().startsWith('#'))
			.join('\n')
			.trim();
		try {
			await navigator.clipboard.writeText(text);
			copied = true;
			setTimeout(() => (copied = false), 1400);
		} catch (e) {}
	}
</script>

<div class="code">
	<div class="head">
		<span class="label">{label}</span>
		<button type="button" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
	</div>
	<pre>{#each code.split('\n') as line}{#if line.trimStart().startsWith('#')}<span class="c">{line}</span>{:else}{line}{/if}{'\n'}{/each}</pre>
</div>

<style>
	.code { background: var(--term); border: 1px solid color-mix(in srgb, var(--term-ink) 10%, transparent); border-radius: 14px; overflow: hidden; color: var(--term-ink); }
	.head { display: flex; align-items: center; justify-content: space-between; padding: 8px 10px 0 16px; }
	.label { font-size: 0.75rem; font-weight: 700; color: var(--term-dim); letter-spacing: 0.04em; }
	button {
		font: 700 0.75rem var(--font);
		color: var(--term-ink);
		background: color-mix(in srgb, var(--term-ink) 10%, transparent);
		border: 0;
		border-radius: 999px;
		padding: 5px 12px;
		cursor: pointer;
	}
	button:hover { background: color-mix(in srgb, var(--term-ink) 18%, transparent); }
	pre { margin: 0; padding: 10px 16px 16px; font-size: 0.84rem; line-height: 1.7; overflow-x: auto; }
	.c { color: var(--term-dim); }
</style>
