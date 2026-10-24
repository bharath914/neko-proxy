<script>
	/** A terminal-style block with a copy button. Lines starting with # are comments and are not copied. */
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
			setTimeout(() => (copied = false), 1600);
		} catch (e) {}
	}
</script>

<div class="code">
	<div class="head">
		<span class="label">{label}</span>
		<button type="button" onclick={copy} class:copied aria-live="polite">{copied ? 'Copied' : 'Copy'}</button>
	</div>
	<pre>{#each code.split('\n') as line}{#if line.trimStart().startsWith('#')}<span class="c">{line}</span>{:else}{line}{/if}{'\n'}{/each}</pre>
</div>

<style>
	.code { background: var(--term); border-radius: 16px; overflow: hidden; color: var(--term-ink); min-width: 0; }
	.head { display: flex; align-items: center; justify-content: space-between; padding: 10px 10px 0 18px; }
	.label { font: 500 0.75rem var(--font); color: var(--term-dim); }
	button {
		font: 500 0.75rem var(--font);
		color: var(--term-ink);
		background: rgb(255 255 255 / 0.1);
		border: 0;
		border-radius: 999px;
		padding: 5px 12px;
		cursor: pointer;
		transition: transform 0.25s cubic-bezier(0.2, 0.9, 0.3, 1), background-color 0.2s;
	}
	button:hover { background: rgb(255 255 255 / 0.16); }
	button:active { transform: scale(0.92); transition-duration: 0.1s; }
	button.copied { background: #32d74b; color: #04210d; }
	pre { margin: 0; padding: 10px 18px 18px; font-size: 0.82rem; line-height: 1.75; overflow-x: auto; }
	.c { color: var(--term-dim); }
</style>
