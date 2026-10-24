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

<section class="block alt" id="phones">
	<div class="wrap">
		<div class="center" use:reveal>
			<span class="eyebrow tint">Phones without the pain</span>
			<h2 class="section-title">Two ways in.<br />Neither is a scavenger hunt.</h2>
			<p class="section-lede">Proxying a phone used to mean Wi-Fi settings, a certificate download, a trust toggle three menus deep, and an app that ignores it anyway. Neko gives you a shortcut for your own app and a guide for everything else.</p>
		</div>

		<div class="two">
			<article class="card way" use:reveal>
				<p class="tags"><span class="pill green">Android · beta</span> <span class="pill amber">iOS · in progress</span></p>
				<h3>Your own app.<br /><span>The in-app SDK.</span></h3>
				<p class="body">Add one Gradle plugin to a debug build. The app finds Neko on its own: emulators and USB phones connect automatically, Wi-Fi phones confirm a six-digit code. No proxy settings, no certificate, and nothing ships in release builds.</p>
				<div class="pair" aria-hidden="true">
					<div class="phone"><span class="notch"></span><small>Pair with Neko?</small><b>482 913</b></div>
					<div class="dots"><span></span><span></span><span></span></div>
					<div class="mac"><small>Galaxy Tab A9 wants to pair</small><b>482 913</b><i>Allow</i></div>
				</div>
				<Code label="Kotlin DSL" code={gradle} />
				<a class="link-arrow" href={`${repo}/blob/main/docs/sdk-quickstart.md`}>SDK quickstart</a>
			</article>

			<article class="card way" use:reveal={100}>
				<p class="tags"><span class="pill">iPhone</span> <span class="pill">Android</span> <span class="pill">macOS</span> <span class="pill">Windows</span> <span class="pill">Linux</span></p>
				<h3>Everything else.<br /><span>neko.local</span></h3>
				<p class="body">Point any device at the proxy and open <code>http://neko.local</code>. Neko serves its own certificate page: the right steps for that platform, PEM, DER and an Apple profile, and a live check that turns green once the device trusts it.</p>
				<div class="browser" aria-hidden="true">
					<div class="b-bar"><span>neko.local</span></div>
					<div class="b-body">
						<p class="b-title">Trust Neko on this iPhone</p>
						<ol>
							<li class="done">Download the profile</li>
							<li class="done">Install it in Settings</li>
							<li class="now">Turn on full trust</li>
						</ol>
						<p class="b-ok">This device trusts Neko</p>
						<p class="b-fp">SHA-256 0956 BDF0 1DE1 1148 …</p>
					</div>
				</div>
				<p class="small">Every install creates its own certificate, and its private key never leaves your computer. That’s why there’s nothing to download from this website.</p>
				<a class="link-arrow" href={resolve('/install-cert')}>Certificate guide for every platform</a>
			</article>
		</div>
	</div>
</section>

<style>
	.two { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; margin-top: 64px; }
	@media (max-width: 960px) { .two { grid-template-columns: 1fr; } }
	.way { padding: 36px; display: flex; flex-direction: column; gap: 20px; min-width: 0; }
	@media (max-width: 560px) { .way { padding: 24px 20px; } }
	.tags { display: flex; flex-wrap: wrap; gap: 6px; }
	.way h3 { font-size: clamp(1.7rem, 3vw, 2.2rem); line-height: 1.1; letter-spacing: -0.028em; }
	.way h3 span { color: var(--ink-2); }
	.body { color: var(--ink-2); }
	.small { font-size: 0.86rem; color: var(--ink-3); }
	.link-arrow { margin-top: auto; }

	.pair { display: flex; align-items: center; justify-content: center; gap: 14px; padding: 22px 12px; border-radius: 20px; background: var(--bg); }
	.phone, .mac { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 5px; text-align: center; background: var(--surface-2); box-shadow: inset 0 0 0 1.5px var(--line-2); }
	.phone { position: relative; width: 104px; height: 170px; border-radius: 22px; }
	.notch { position: absolute; top: 9px; width: 30px; height: 7px; border-radius: 6px; background: var(--line-2); }
	.mac { width: 196px; height: 124px; border-radius: 12px; padding: 8px; }
	.pair small { font-size: 0.68rem; color: var(--ink-2); font-weight: 500; }
	.pair b { font: 600 1rem var(--mono); white-space: nowrap; }
	.pair i { font-style: normal; font-size: 0.72rem; font-weight: 600; background: var(--blue); color: var(--on-blue); border-radius: 6px; padding: 2px 12px; }
	.dots { display: flex; gap: 5px; }
	.dots span { width: 6px; height: 6px; border-radius: 50%; background: var(--blue); opacity: 0.35; }
	.dots span:nth-child(2) { opacity: 0.6; } .dots span:nth-child(3) { opacity: 1; }

	.browser { border-radius: 16px; overflow: hidden; background: var(--bg); box-shadow: inset 0 0 0 1px var(--line); }
	.b-bar { padding: 9px; display: grid; place-items: center; background: var(--surface-2); }
	.b-bar span { font-size: 0.78rem; padding: 3px 18px; border-radius: 8px; background: var(--fill); color: var(--ink-2); }
	.b-body { padding: 20px 22px; }
	.b-title { font-weight: 600; font-size: 1.05rem; letter-spacing: -0.016em; }
	ol { margin: 14px 0; padding: 0; list-style: none; display: grid; gap: 8px; font-size: 0.92rem; }
	li { display: flex; align-items: center; gap: 10px; color: var(--ink-2); }
	li::before { content: ''; width: 18px; height: 18px; border-radius: 50%; flex: none; box-shadow: inset 0 0 0 1.5px var(--line-2); }
	li.done::before { background: var(--green) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='3.4' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M6 12.5l4 4L18 8'/%3E%3C/svg%3E") center / 11px no-repeat; box-shadow: none; }
	li.now { color: var(--ink); font-weight: 500; }
	li.now::before { box-shadow: inset 0 0 0 2px var(--blue); }
	.b-ok { font-weight: 600; color: var(--green); font-size: 0.92rem; }
	.b-fp { font: 0.74rem var(--mono); color: var(--ink-3); margin-top: 4px; }
</style>
