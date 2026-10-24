<script>
	import { reveal } from '#lib/reveal.js';
	import Code from './Code.svelte';

	const read = ['list_flows', 'search_flows', 'get_flow', 'diff_flows', 'export_har'];
	const write = ['create_mock', 'add_breakpoint', 'replay_request', 'start_capture', 'stop_capture'];
</script>

<!-- Always dark: a stage for the terminal, whatever the page theme. -->
<section class="block ai" id="ai">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow tint">AI-ready</span>
			<h2 class="section-title">Your assistant can<br />finally see the wire.</h2>
			<p class="section-lede">
				Neko ships an MCP server inside the core. Point Claude Code, or any MCP client, at it and it can search your
				captured traffic, explain a failing call, diff two requests or set up a mock, while you stay in charge.
			</p>
		</div>

		<div class="demo" use:reveal={100}>
			<div class="term">
				<div class="t-bar"><span></span><span></span><span></span><b>claude — ~/shop-app</b></div>
				<div class="t-body">
					<p class="you">› Checkout fails on the Pixel. What’s going on?</p>
					<p class="call">⏺ proxy · <b>search_flows</b> <i>status:5xx host:dummyjson.com</i></p>
					<p class="out">⎿ 1 flow · POST /carts/add → <em class="bad">503</em> in 301 ms</p>
					<p class="call">⏺ proxy · <b>get_flow</b> <i>12</i></p>
					<p class="out">⎿ matched rule <em>“Checkout is down”</em> (mock) · server never contacted<br />&nbsp;&nbsp;authorization: <em class="bad">[redacted]</em></p>
					<p class="say">The 503 isn’t from your backend: a local mock rule named “Checkout is down” is answering every <code>POST /carts/add</code>. Want me to replay the request against the real server to compare?</p>
					<p class="you">› Yes, replay it.</p>
					<p class="call">⏺ proxy · <b>replay_request</b> <i>12</i></p>
					<p class="out wait">⎿ <span class="spin"></span> waiting for approval in Neko…</p>
				</div>
			</div>

			<div class="approval" aria-label="Approval card in the Neko app">
				<span class="badge">AI client</span>
				<p class="a-title">Replay POST dummyjson.com/carts/add</p>
				<p class="a-sub">› Exact request</p>
				<div class="a-btns"><span class="allow">✓ Allow</span><span class="deny">Deny</span></div>
			</div>
		</div>
		<p class="example">An example session. The tool names and the approval card are real.</p>

		<div class="facts">
			<div use:reveal>
				<h3>Reading is free.</h3>
				<p>{#each read as t}<code>{t}</code>{/each}</p>
			</div>
			<div use:reveal={60}>
				<h3>Changes wait for your Allow.</h3>
				<p>{#each write as t}<code>{t}</code>{/each}</p>
			</div>
			<div use:reveal={120}>
				<h3>Secrets never leave.</h3>
				<p class="txt">Authorization, cookies, API keys and token-shaped values are redacted in the core, and captured bodies are marked as untrusted data, so a response can’t steer your assistant.</p>
			</div>
			<div use:reveal={180}>
				<h3>Loopback and a token.</h3>
				<p class="txt">The server listens on 127.0.0.1:8898 and rejects anyone without the bearer token. One command prints the setup line:</p>
			</div>
		</div>

		<div class="setup" use:reveal>
			<Code label="terminal" code={'nekod mcp\n# claude mcp add --transport http proxy http://127.0.0.1:8898/mcp \\\n#   --header "Authorization: Bearer …"'} />
		</div>
	</div>
</section>

<style>
	.ai {
		--bg: #000;
		--ink: #f5f5f7;
		--ink-2: #a1a1a6;
		--ink-3: #86868b;
		--fill: rgb(120 120 128 / 0.24);
		--line-2: rgb(255 255 255 / 0.16);
		--blue: #3b8bff;
		--surface: #1c1c1e;
		--surface-2: #2c2c2e;
		background: radial-gradient(60% 40% at 50% 30%, rgb(59 139 255 / 0.18), transparent 70%), #000;
		color: var(--ink);
		overflow: hidden;
	}
	.ai :global(:not(pre) > code) { background: rgb(255 255 255 / 0.1); color: #f5f5f7; }

	.demo { position: relative; max-width: 46rem; margin: 64px auto 0; }
	.term { border-radius: 18px; overflow: hidden; background: #0b0d12; box-shadow: 0 0 0 0.5px rgb(255 255 255 / 0.14), 0 40px 120px rgb(0 0 0 / 0.8); font: 0.84rem/1.7 var(--mono); color: #e6e9ef; overflow-wrap: anywhere; }
	.t-bar { display: flex; align-items: center; gap: 7px; padding: 11px 14px; background: #15181f; }
	.t-bar span { width: 11px; height: 11px; border-radius: 50%; background: rgb(255 255 255 / 0.18); }
	.t-bar b { flex: 1; text-align: center; margin-right: 50px; font: 500 0.75rem var(--font); color: #8a90a0; }
	.t-body { padding: 20px 22px 24px; display: grid; gap: 8px; }
	.you { color: #fff; font-weight: 600; }
	.call { color: #9ec2ff; }
	.call b { color: #d4e3ff; font-weight: 600; }
	.call i { font-style: normal; color: #8a90a0; }
	.out { color: #8a90a0; }
	.out em { font-style: normal; color: #ffd28a; }
	.out em.bad { color: #ff8a92; }
	.say { font: 1rem/1.5 var(--font); letter-spacing: -0.01em; color: #e6e9ef; padding: 6px 0; }
	.wait { display: flex; align-items: center; gap: 8px; color: #ffd28a; }
	.spin { width: 12px; height: 12px; border-radius: 50%; border: 2px solid currentColor; border-right-color: transparent; animation: spin 0.9s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	/* The approval card materializes: blur and scale arrive together. */
	.approval {
		position: relative;
		width: min(330px, 88%);
		margin: -64px -28px 0 auto;
		padding: 18px;
		border-radius: 18px;
		background: rgb(40 40 44 / 0.72);
		backdrop-filter: blur(30px) saturate(1.8);
		-webkit-backdrop-filter: blur(30px) saturate(1.8);
		box-shadow: 0 0 0 1.5px #3b8bff, inset 0 1px 0 rgb(255 255 255 / 0.1), 0 30px 80px rgb(0 0 0 / 0.7);
	}
	:global(.demo.reveal:not(.shown)) .approval { opacity: 0; }
	:global(.demo.shown) .approval { animation: materialize 0.7s 1.1s cubic-bezier(0.2, 0.9, 0.3, 1) both; }
	@keyframes materialize { from { opacity: 0; transform: scale(0.92) translateY(10px); filter: blur(8px); } to { opacity: 1; transform: none; filter: none; } }
	.badge { font-size: 0.72rem; font-weight: 600; padding: 2px 8px; border-radius: 6px; background: rgb(59 139 255 / 0.22); color: #8ab8ff; }
	.a-title { font-weight: 600; margin: 10px 0 4px; letter-spacing: -0.014em; }
	.a-sub { color: #a1a1a6; font-size: 0.86rem; }
	.a-btns { display: flex; gap: 8px; margin-top: 14px; }
	.a-btns span { padding: 6px 16px; border-radius: 9px; font-weight: 600; font-size: 0.88rem; }
	.allow { background: #0a6cff; color: #fff; }
	.deny { background: rgb(255 255 255 / 0.12); color: #fff; }
	.example { text-align: center; margin-top: 22px; font-size: 0.82rem; color: var(--ink-3); }
	@media (max-width: 560px) { .approval { margin-right: 8px; } }

	.facts { display: grid; grid-template-columns: repeat(4, 1fr); gap: 32px; margin-top: 96px; }
	@media (max-width: 960px) { .facts { grid-template-columns: 1fr 1fr; } }
	@media (max-width: 540px) { .facts { grid-template-columns: 1fr; } }
	.facts > div { min-width: 0; border-top: 1px solid var(--line-2); padding-top: 20px; }
	.facts h3 { font-size: 1.15rem; margin-bottom: 10px; }
	.facts p { display: flex; flex-wrap: wrap; gap: 6px; }
	.facts p.txt { display: block; color: var(--ink-2); font-size: 0.95rem; }
	.facts code { font-size: 0.78rem; }
	.setup { max-width: 46rem; margin: 40px auto 0; }
</style>
