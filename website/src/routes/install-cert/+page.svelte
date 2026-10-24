<script>
	import { onMount } from 'svelte';
	import { asset } from '$app/paths';
	import Code from '#lib/components/Code.svelte';

	const names = { ios: 'iPhone / iPad', android: 'Android', mac: 'macOS', win: 'Windows', linux: 'Linux', firefox: 'Firefox' };
	let os = $state('ios');

	// Pick a tab from the browser (or the URL hash); the visitor can switch.
	onMount(() => {
		const h = location.hash.slice(1);
		if (names[h]) return void (os = h);
		const ua = navigator.userAgent || '';
		os = /iPhone|iPad|iPod/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1) ? 'ios'
			: /Android/.test(ua) ? 'android'
			: /Firefox\//.test(ua) && !/Mobile|Tablet/.test(ua) ? 'firefox'
			: /Windows/.test(ua) ? 'win'
			: /Mac OS X|Macintosh/.test(ua) ? 'mac'
			: /Linux|X11|CrOS/.test(ua) ? 'linux' : 'ios';
	});

	function pick(k) {
		os = k;
		try { history.replaceState(history.state, '', '#' + k); } catch (e) {}
	}
</script>

<svelte:head>
	<title>Install the Neko certificate</title>
	<meta name="description" content="Let Neko Proxy read HTTPS: install its certificate on iPhone, Android, macOS, Windows, Linux or Firefox." />
</svelte:head>

<header class="top">
	<div class="wrap narrow">
		<img class="cat" src={asset('/assets/neko.png')} alt="" width="104" height="104" />
		<h1>Teach your device to trust Neko</h1>
		<p class="lede">HTTPS is encrypted. For Neko to show it to you, your device has to trust Neko’s certificate. It takes about a minute.</p>
	</div>
</header>

<div class="wrap narrow body">
	<section class="card why">
		<h2>First: why isn’t the certificate on this page?</h2>
		<p>Every Neko install makes its <b>own</b> certificate, and its private key never leaves your computer. So there’s nothing to download from this website, and that’s on purpose. Your device gets the certificate straight from your own Neko:</p>
		<span class="big-url">http://neko.local</span>
		<p class="muted">That address only works on a device that is already sending its traffic through Neko. Do step 1 first.</p>
	</section>

	<section>
		<h2><span class="step">1</span> Send the device through Neko</h2>
		<div class="card pad">
			<ol>
				<li>Start Neko on your computer. The proxy listens on port <code>8899</code>.</li>
				<li>Set the device’s proxy to <b>Manual</b>, host <code>127.0.0.1</code>, port <code>8899</code>. For an emulator use the host’s address (Android: <code>10.0.2.2</code>).</li>
			</ol>
			<p class="muted">Neko listens on your own machine only. Phone-over-Wi-Fi (LAN mode) is still being built, so for now use the same computer, an emulator, or the in-app SDK for phones.</p>
		</div>
	</section>

	<section>
		<h2><span class="step">2</span> Install the certificate</h2>
		<p class="sub">On the device, open <code>http://neko.local</code> in its browser. Or follow the steps for your platform here.</p>
		<div class="tabs" role="tablist" aria-label="Platform">
			{#each Object.entries(names) as [k, n]}
				<button type="button" role="tab" aria-selected={os === k} onclick={() => pick(k)}>{n}</button>
			{/each}
		</div>

		<div class="card pad panel">
			{#if os === 'ios'}
				<p>Use <b>Safari</b>. Other iPhone browsers can’t install profiles.</p>
				<ol>
					<li>Open <code>http://neko.local/ca.mobileconfig</code> and tap <b>Allow</b>.</li>
					<li>Open <b>Settings</b> → <b>Profile Downloaded</b> (or <b>General → VPN &amp; Device Management</b>), then <b>Install</b>.</li>
					<li><b>Then turn on full trust:</b> Settings → General → About → <b>Certificate Trust Settings</b>, and switch Neko on. Without this, HTTPS still fails.</li>
				</ol>
			{:else if os === 'android'}
				<ol>
					<li>Open <code>http://neko.local/ca.crt</code> to download it.</li>
					<li>Settings → search <b>CA certificate</b> (often <b>Security → Encryption &amp; credentials → Install a certificate</b>) → <b>CA certificate</b>.</li>
					<li>Pick <code>neko-ca.pem</code> from Downloads.</li>
				</ol>
				<div class="note"><b>Apps may ignore it.</b> Since Android 7, apps only trust user certificates if their developers allow it. Chrome works. For your own app, use Neko’s in-app SDK, which handles trust for you, or allow user certificates in debug builds.</div>
			{:else if os === 'mac'}
				<p>Neko’s Mac app has an <b>Install</b> button in Settings → Certificate. By hand:</p>
				<Code code={'curl -o ~/Downloads/neko-ca.pem http://neko.local/ca.pem\nsudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ~/Downloads/neko-ca.pem'} />
			{:else if os === 'win'}
				<ol>
					<li>Open <code>http://neko.local/ca.cer</code> to download it.</li>
					<li>Open the file → <b>Install Certificate</b> → <b>Local Machine</b> → <b>Place all certificates in the following store</b> → <b>Trusted Root Certification Authorities</b>.</li>
					<li>Finish and restart your browser.</li>
				</ol>
				<p>Or from an administrator Command Prompt:</p>
				<Code code={'certutil -addstore -f Root %USERPROFILE%\\Downloads\\neko-ca.cer'} />
			{:else if os === 'linux'}
				<p>First download it, then pick your distro:</p>
				<Code code={'curl -o ~/Downloads/neko-ca.pem http://neko.local/ca.pem'} />
				<h3>Fedora, RHEL</h3>
				<Code code={'sudo cp ~/Downloads/neko-ca.pem /etc/pki/ca-trust/source/anchors/ && sudo update-ca-trust'} />
				<h3>Debian, Ubuntu</h3>
				<Code code={'sudo cp ~/Downloads/neko-ca.pem /usr/local/share/ca-certificates/neko-ca.crt && sudo update-ca-certificates'} />
				<h3>Arch</h3>
				<Code code={'sudo trust anchor --store ~/Downloads/neko-ca.pem'} />
				<p class="muted">Chrome, Chromium and Firefox keep their own store. See the Firefox tab.</p>
			{:else}
				<p>Firefox ignores the system’s certificates by default.</p>
				<ol>
					<li>Download <code>http://neko.local/ca.pem</code>.</li>
					<li>Open <code>about:preferences#privacy</code> → <b>View Certificates</b> → <b>Authorities</b> → <b>Import</b>.</li>
					<li>Tick <b>Trust this CA to identify websites</b>.</li>
				</ol>
			{/if}
		</div>
	</section>

	<section>
		<h2><span class="step">3</span> Check the fingerprint</h2>
		<div class="card pad">
			<p>On <code>http://neko.local</code> the page shows the certificate’s SHA-256 fingerprint and turns green once your device trusts it. Compare the fingerprint with <b>Settings → Certificate</b> in the Neko app. If they differ, <b>don’t install</b>: you aren’t talking to your own proxy.</p>
		</div>
	</section>

	<section>
		<h2>Questions</h2>
		<div class="card faq">
			<details open><summary>Is it safe?</summary><p>The private key was made on your computer and stays there. It isn’t in the file you download. Anyone with the key could read your HTTPS traffic, so only install a certificate from a Neko you run yourself, and remove it when you’re done.</p></details>
			<details><summary>What can’t Neko see?</summary><p>Apps that pin their certificate refuse Neko’s. Neko notices, passes that traffic through untouched, and flags it in the app.</p></details>
			<details><summary>It still says “not trusted”</summary><ul><li><b>iPhone:</b> did you switch on full trust under Certificate Trust Settings?</li><li><code>neko.local</code> doesn’t load: the device isn’t using the proxy. Recheck the proxy address and that you’re on the same network.</li><li>The device’s date is wrong: turn on automatic date and time.</li><li>Only one app fails: it likely pins its certificate or ignores user certificates.</li></ul></details>
			<details><summary>How do I remove it?</summary><p><b>iPhone:</b> Settings → General → VPN &amp; Device Management → profile → Remove.<br /><b>Android:</b> Settings → Security → Encryption &amp; credentials → Trusted credentials → User → remove.<br /><b>macOS:</b> Keychain Access → System → delete the Neko certificate.<br /><b>Windows:</b> <code>certmgr.msc</code> → Trusted Root Certification Authorities → delete it.<br /><b>Linux:</b> delete the file you copied, then run <code>update-ca-trust</code> or <code>update-ca-certificates --fresh</code>.</p></details>
		</div>
	</section>
</div>

<style>
	.narrow { max-width: 46rem; }
	.top { text-align: center; padding: 64px 0 48px; }
	.cat { width: 96px; height: 96px; border-radius: 22px; margin: 0 auto; box-shadow: 0 10px 30px color-mix(in srgb, var(--blue) 30%, transparent); }
	h1 { font-size: clamp(2.2rem, 6vw, 3.6rem); margin: 24px 0 14px; }
	.lede { color: var(--ink-2); font-size: clamp(1.1rem, 2vw, 1.3rem); line-height: 1.4; letter-spacing: -0.016em; max-width: 34rem; margin: 0 auto; }
	.body { display: grid; gap: 56px; padding-bottom: 96px; }
	h2 { display: flex; align-items: center; gap: 12px; font-size: 1.6rem; letter-spacing: -0.024em; margin-bottom: 16px; }
	.step { width: 32px; height: 32px; border-radius: 50%; display: grid; place-items: center; background: var(--blue); color: var(--on-blue); font-size: 0.95rem; font-weight: 600; flex: none; }
	h3 { font-size: 1rem; margin: 18px 0 8px; }
	.card { background: var(--bg-alt); }
	.card.why, .pad { padding: 28px; }
	.why h2 { font-size: 1.3rem; }
	.why p, .pad p, .panel li { color: var(--ink-2); }
	.big-url { display: block; text-align: center; font: 600 clamp(1.4rem, 5vw, 2rem) var(--font-display); letter-spacing: -0.02em; background: var(--blue-soft); color: var(--blue); border-radius: 16px; padding: 18px; margin: 18px 0; word-break: break-all; }
	.muted { color: var(--ink-3) !important; font-size: 0.9rem; margin-top: 10px; }
	.sub { color: var(--ink-2); margin-bottom: 18px; }
	ol, ul { padding-left: 1.3rem; margin: 0 0 8px; display: grid; gap: 8px; }
	.panel { display: grid; gap: 12px; }
	.panel > p { margin: 0; }
	.tabs { display: inline-flex; gap: 2px; padding: 3px; margin-bottom: 16px; border-radius: 999px; background: var(--fill); max-width: 100%; overflow-x: auto; scrollbar-width: none; }
	.tabs button { flex: none; font: 500 0.9rem var(--font); letter-spacing: -0.01em; padding: 7px 15px; border-radius: 999px; border: 0; background: none; color: var(--ink-2); cursor: pointer; transition: background-color 0.2s, color 0.2s, transform 0.25s cubic-bezier(0.2, 0.9, 0.3, 1); }
	.tabs button[aria-selected='true'] { background: var(--surface); color: var(--ink); box-shadow: 0 1px 3px rgb(0 0 0 / 0.12); }
	.tabs button:active { transform: scale(0.95); transition-duration: 0.1s; }
	.note { background: var(--amber-soft); color: var(--amber); border-radius: 14px; padding: 14px 16px; font-size: 0.93rem; }
	.faq { padding: 4px 28px; }
	details { border-top: 1px solid var(--line); padding: 18px 0; }
	details:first-child { border-top: 0; }
	summary { cursor: pointer; font-weight: 600; letter-spacing: -0.014em; }
	details p, details ul { margin: 10px 0 0; color: var(--ink-2); }
</style>
