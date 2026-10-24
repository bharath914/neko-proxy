<script>
	import { resolve } from '$app/paths';
	import { reveal } from '#lib/reveal.js';
	import { sdkMaven, repo } from '#lib/site.js';
	import Code from './Code.svelte';

	const gradle = `// settings.gradle.kts: add the Neko repository
pluginManagement {
    repositories { maven("${sdkMaven}"); google(); mavenCentral(); gradlePluginPortal() }
}
dependencyResolutionManagement {
    repositories { maven("${sdkMaven}"); google(); mavenCentral() }
}

// app/build.gradle.kts: one plugin, debug builds only
plugins {
    id("com.android.application")
    id("dev.nekoproxy.inspector") version "0.1.0"
}`;
</script>

<section class="block phones" id="phones">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow">Phones without the pain</span>
			<h2 class="section-title">Two ways in. Neither involves a scavenger hunt.</h2>
			<p class="section-lede">Proxying a phone used to mean Wi-Fi settings, a certificate download, a trust toggle hidden three menus deep, and an app that ignores it anyway. Neko gives you a shortcut for your own app and a guide for everything else.</p>
		</div>

		<div class="two">
			<article class="card way" use:reveal>
				<div class="way-head">
					<span class="num">1</span>
					<div>
						<h3>Your own app: the in-app SDK</h3>
						<p class="tags"><span class="pill green">Android · beta</span> <span class="pill amber">iOS · in progress</span></p>
					</div>
				</div>
				<p>Add one Gradle plugin to a debug build. The app finds Neko on its own: emulators and USB phones connect automatically, Wi-Fi phones confirm a six-digit code. No proxy settings, no certificate to install, and nothing ships in release builds.</p>
				<div class="pair" aria-hidden="true">
					<div class="device phone"><span class="scr">Neko · pair?<b>482 913</b></span></div>
					<div class="link"><span></span><span></span><span></span></div>
					<div class="device desk"><span class="scr">Galaxy Tab A9 wants to pair<b>482 913</b><i>Allow</i></span></div>
				</div>
				<Code label="Kotlin DSL" code={gradle} />
				<a class="more" href={`${repo}/blob/main/docs/sdk-quickstart.md`}>SDK quickstart →</a>
			</article>

			<article class="card way" use:reveal={100}>
				<div class="way-head">
					<span class="num">2</span>
					<div>
						<h3>Everything else: neko.local</h3>
						<p class="tags"><span class="pill">iPhone</span> <span class="pill">Android</span> <span class="pill">macOS</span> <span class="pill">Windows</span> <span class="pill">Linux</span></p>
					</div>
				</div>
				<p>Point any device at the proxy and open <code>http://neko.local</code>. Neko serves its own certificate page: the right steps for that platform, PEM, DER and an Apple profile, and a live check that turns green once the device trusts it.</p>
				<div class="browser" aria-hidden="true">
					<div class="b-bar"><span class="b-url">🔒 neko.local</span></div>
					<div class="b-body">
						<p class="b-title">Trust Neko on this iPhone</p>
						<ol>
							<li class="done">Download the profile</li>
							<li class="done">Install it in Settings</li>
							<li class="now">Turn on full trust</li>
						</ol>
						<p class="b-ok"><span></span> This device trusts Neko</p>
						<p class="b-fp">SHA-256 0956 BDF0 1DE1 1148 …</p>
					</div>
				</div>
				<p class="small">Every install creates its own certificate, and its private key never leaves your computer. That’s why there’s nothing to download from this website.</p>
				<a class="more" href={resolve('/install-cert')}>Certificate guide for every platform →</a>
			</article>
		</div>
	</div>
</section>

<style>
	.two { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; margin-top: 48px; }
	@media (max-width: 960px) { .two { grid-template-columns: 1fr; } }
	.way { padding: 28px; display: flex; flex-direction: column; gap: 18px; min-width: 0; }
	@media (max-width: 560px) { .way { padding: 20px; } }
	.way-head { display: flex; gap: 16px; align-items: flex-start; }
	.num { flex: none; width: 40px; height: 40px; border-radius: 12px; display: grid; place-items: center; background: linear-gradient(135deg, var(--blue), var(--pink)); color: #fff; font-weight: 900; }
	.way h3 { font-size: 1.35rem; margin-bottom: 8px; }
	.tags { display: flex; flex-wrap: wrap; gap: 6px; }
	.way > p { color: var(--ink-2); }
	.small { font-size: 0.88rem; color: var(--ink-3) !important; }
	.more { font-weight: 800; text-decoration: none; margin-top: auto; }

	.pair { display: flex; align-items: center; justify-content: center; gap: 12px; padding: 18px; border-radius: var(--r); background: var(--surface-2); }
	.device { border: 2px solid var(--line-2); background: var(--surface); display: grid; place-items: center; text-align: center; box-shadow: var(--shadow-sm); }
	.phone { width: 100px; height: 160px; border-radius: 18px; }
	.desk { width: 190px; height: 120px; border-radius: 10px; }
	.scr { display: flex; flex-direction: column; gap: 4px; font-size: 0.7rem; font-weight: 700; color: var(--ink-3); padding: 8px; }
	.scr b { font: 600 0.95rem var(--mono); color: var(--ink); white-space: nowrap; }
	.scr i { font-style: normal; align-self: center; background: var(--blue); color: var(--on-blue); border-radius: 6px; padding: 1px 10px; font-weight: 800; }
	.link { display: flex; gap: 5px; }
	.link span { width: 7px; height: 7px; border-radius: 50%; background: var(--blue); animation: hop 1.2s ease-in-out infinite; }
	.link span:nth-child(2) { animation-delay: 0.15s; } .link span:nth-child(3) { animation-delay: 0.3s; }
	@keyframes hop { 50% { transform: translateY(-5px); opacity: 0.4; } }

	.browser { border-radius: var(--r); border: 1px solid var(--line-2); overflow: hidden; background: var(--surface); box-shadow: var(--shadow-sm); }
	.b-bar { padding: 8px; background: var(--surface-2); border-bottom: 1px solid var(--line); display: grid; place-items: center; }
	.b-url { font: 500 0.78rem var(--mono); background: var(--surface); padding: 3px 14px; border-radius: 999px; border: 1px solid var(--line); color: var(--ink-2); }
	.b-body { padding: 18px 20px; }
	.b-title { font-weight: 900; font-size: 1.05rem; }
	ol { margin: 12px 0; padding: 0; list-style: none; display: grid; gap: 6px; font-size: 0.9rem; font-weight: 700; }
	li { display: flex; align-items: center; gap: 10px; color: var(--ink-2); }
	li::before { content: ''; width: 18px; height: 18px; border-radius: 50%; border: 2px solid var(--line-2); flex: none; }
	li.done::before { background: var(--green); border-color: var(--green); box-shadow: inset 0 0 0 3px var(--surface); }
	li.now { color: var(--ink); }
	li.now::before { border-color: var(--blue); animation: ring 1.6s ease-in-out infinite; }
	@keyframes ring { 50% { box-shadow: 0 0 0 5px color-mix(in srgb, var(--blue) 20%, transparent); } }
	.b-ok { display: flex; align-items: center; gap: 8px; font-weight: 800; color: var(--green); font-size: 0.92rem; }
	.b-ok span { width: 9px; height: 9px; border-radius: 50%; background: var(--green); }
	.b-fp { font: 500 0.74rem var(--mono); color: var(--ink-3); margin-top: 6px; }
</style>
