<script>
	import { asset } from '$app/paths';
	import Hero from '#lib/components/Hero.svelte';
	import Why from '#lib/components/Why.svelte';
	import Principles from '#lib/components/Principles.svelte';
	import Tour from '#lib/components/Tour.svelte';
	import Lifecycle from '#lib/components/Lifecycle.svelte';
	import Numbers from '#lib/components/Numbers.svelte';
	import AI from '#lib/components/AI.svelte';
	import Phones from '#lib/components/Phones.svelte';
	import Roadmap from '#lib/components/Roadmap.svelte';
	import Download from '#lib/components/Download.svelte';

	const description =
		'Neko Proxy is a fast, free, open-source HTTP and HTTPS debugging proxy with a Rust core, native apps, an in-app mobile SDK and a built-in MCP server for AI assistants.';
</script>

<svelte:head>
	<title>Neko Proxy</title>
	<meta name="description" content={description} />
	<meta property="og:title" content="Neko Proxy: see every request, change any of them" />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="website" />
	<meta property="og:image" content="https://bharath914.github.io/neko-proxy/shots/inspector-light.jpg" />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="theme-color" content="#ffffff" media="(prefers-color-scheme: light)" />
	<meta name="theme-color" content="#000000" media="(prefers-color-scheme: dark)" />
</svelte:head>

<Hero />
<Why />
<Principles />
<Tour />
<Lifecycle />
<Numbers />
<AI />
<Phones />
<Roadmap />
<Download />
