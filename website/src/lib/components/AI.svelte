<script>
	import { reveal } from '#lib/reveal.js';
	import Code from './Code.svelte';

	const read = ['list_flows', 'search_flows', 'get_flow', 'diff_flows', 'export_har'];
	const write = ['create_mock', 'add_breakpoint', 'replay_request', 'start_capture', 'stop_capture'];
</script>

<section class="block ai" id="ai">
	<div class="wrap grid">
		<div class="copy" use:reveal>
			<span class="eyebrow">AI-ready</span>
			<h2 class="section-title">Your assistant can finally see the wire.</h2>
			<p class="section-lede">
				Neko ships an MCP server inside the core. Point Claude Code (or any MCP client) at it, and it can search
				your captured traffic, explain a failing call, diff two requests or set up a mock, while you stay in charge.
			</p>

			<div class="tools">
				<div>
					<h3><span class="pill green">Read</span> runs freely</h3>
					<p>{#each read as t}<code>{t}</code>{/each}</p>
				</div>
				<div>
					<h3><span class="pill amber">Change</span> waits for your Allow</h3>
					<p>{#each write as t}<code>{t}</code>{/each}</p>
				</div>
			</div>

			<ul class="safe">
				<li><b>Secrets are redacted in the core.</b> Authorization, cookies, API keys and token-shaped values never leave it.</li>
				<li><b>Traffic is data, not instructions.</b> Captured bodies are marked untrusted, so a response can’t steer your assistant.</li>
				<li><b>Loopback only, with a token.</b> The server listens on 127.0.0.1:8898 and rejects anyone without the bearer token.</li>
			</ul>

			<Code label="one command prints the setup line" code={'nekod mcp\n# claude mcp add --transport http proxy http://127.0.0.1:8898/mcp \\\n#   --header "Authorization: Bearer …"'} />
		</div>

		<div class="demo" use:reveal={120}>
			<div class="term">
				<div class="t-bar"><span></span><span></span><span></span><b>claude — ~/shop-app</b></div>
				<div class="t-body">
					<p class="you">› Checkout fails on the Pixel. What’s going on?</p>
					<p class="call">⏺ proxy · <b>search_flows</b> <i>status:5xx host:dummyjson.com</i></p>
					<p class="out">⎿ 1 flow · POST /carts/add → <em class="bad">503</em> in 301 ms</p>
					<p class="call">⏺ proxy · <b>get_flow</b> <i>12</i></p>
					<p class="out">⎿ matched rule <em>“Checkout is down”</em> (mock) · server never contacted<br />&nbsp;&nbsp;authorization: <em class="red">[redacted]</em></p>
					<p class="say">The 503 isn’t from your backend: a local mock rule named “Checkout is down” is answering every <code>POST /carts/add</code>. Want me to replay the request against the real server to compare?</p>
					<p class="you">› Yes, replay it.</p>
					<p class="call">⏺ proxy · <b>replay_request</b> <i>12</i></p>
					<p class="out wait">⎿ <span class="spin"></span> waiting for approval in Neko…</p>
				</div>
			</div>

			<div class="approval" aria-label="Approval card in the Neko app">
				<span class="pill">AI client</span>
				<p class="a-title">Replay POST dummyjson.com/carts/add</p>
				<p class="a-sub">› Exact request</p>
				<div class="a-btns"><span class="allow">✓ Allow</span><span class="deny">Deny</span></div>
			</div>
			<p class="example">An example session. The tool names and the approval card are real.</p>
		</div>
	</div>
</section>

<style>
	.ai { background: radial-gradient(60% 50% at 80% 40%, var(--glow-a), transparent 70%); }
	.grid { display: grid; grid-template-columns: 1fr 1fr; gap: 56px; align-items: center; }
	@media (max-width: 960px) { .grid { grid-template-columns: 1fr; } }
	.grid > * { min-width: 0; }

	.tools { display: grid; gap: 16px; margin: 28px 0; }
	.tools h3 { font-size: 0.98rem; display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
	.tools p { display: flex; flex-wrap: wrap; gap: 6px; }
	.tools code { font-size: 0.8rem; }
	.safe { list-style: none; margin: 0 0 24px; padding: 0; display: grid; gap: 10px; }
	.safe li { color: var(--ink-2); font-size: 0.96rem; padding-left: 28px; position: relative; }
	.safe li::before { content: '🛡️'; position: absolute; left: 0; top: 0; font-size: 0.95rem; }
	.safe b { color: var(--ink); }

	.demo { position: relative; }
	.term { border-radius: 16px; overflow-wrap: anywhere; overflow: hidden; background: var(--term); color: var(--term-ink); box-shadow: var(--shadow-lg); border: 1px solid color-mix(in srgb, var(--term-ink) 10%, transparent); font: 0.84rem/1.65 var(--mono); }
	.t-bar { display: flex; align-items: center; gap: 7px; padding: 10px 14px; background: var(--term-2); }
	.t-bar span { width: 11px; height: 11px; border-radius: 50%; background: color-mix(in srgb, var(--term-ink) 20%, transparent); }
	.t-bar b { flex: 1; text-align: center; margin-right: 50px; font: 600 0.75rem var(--font); color: var(--term-dim); }
	.t-body { padding: 18px 20px 22px; display: grid; gap: 8px; }
	.t-body p { margin: 0; }
	.you { color: #fff; font-weight: 600; }
	.call { color: #9ec2ff; }
	.call b { color: #c9dbff; font-weight: 600; }
	.call i { font-style: normal; color: var(--term-dim); }
	.out { color: var(--term-dim); padding-left: 2px; }
	.out em { font-style: normal; color: #ffd28a; }
	.out em.bad, .out em.red { color: #ff8a92; }
	.say { font-family: var(--font); font-size: 0.95rem; line-height: 1.55; color: var(--term-ink); padding: 4px 0; }
	.say code { background: color-mix(in srgb, var(--term-ink) 10%, transparent); border: 0; color: #fff; }
	.wait { display: flex; align-items: center; gap: 8px; color: #ffd28a; }
	.spin { width: 12px; height: 12px; border-radius: 50%; border: 2px solid currentColor; border-right-color: transparent; animation: spin 0.9s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.approval {
		position: relative;
		margin: -56px -18px 0 auto;
		width: min(320px, 86%);
		padding: 16px;
		border-radius: 14px;
		background: var(--surface);
		border: 1.5px solid var(--blue);
		box-shadow: var(--shadow-lg);
	}
	:global(.demo.reveal:not(.shown)) .approval { opacity: 0; }
	:global(.demo.shown) .approval { animation: pop 0.6s 1.2s cubic-bezier(0.2, 0.8, 0.2, 1) both; }
	@keyframes pop { from { opacity: 0; transform: translateY(16px) scale(0.96); } to { opacity: 1; transform: none; } }
	.a-title { font-weight: 900; margin: 10px 0 4px; font-size: 0.98rem; }
	.a-sub { color: var(--ink-3); font-size: 0.86rem; }
	.a-btns { display: flex; gap: 8px; margin-top: 12px; }
	.a-btns span { padding: 6px 14px; border-radius: 8px; font-weight: 800; font-size: 0.88rem; }
	.allow { background: var(--blue); color: var(--on-blue); }
	.deny { background: var(--surface-2); color: var(--ink); }
	.example { margin-top: 14px; font-size: 0.82rem; color: var(--ink-3); }
	@media (max-width: 560px) { .approval { margin-right: 8px; } }
</style>
