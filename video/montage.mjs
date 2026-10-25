// Montage renderer: cuts source pages (each a pure function of time) into one timeline with short dips to black.
// node montage.mjs <audio.wav> <out.mp4> [crf]
import { chromium } from 'playwright';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';
const [, , audio, out, crf = '17'] = process.argv;
// [page, source start, source end]
const CUTS = [
	['opener.html', 0, 5],
	['concept.html', 10.9, 14.9], // composer: type, send, tests
	['concept.html', 3.0, 7.0], // agent writes rules, sets breakpoint
	['concept.html', 8.75, 10.75], // requests
	['hook.html', 2.4, 6.4], // composer, real screenshots
	['concept.html', 15.65, 18.3], // native, fast, simple
	['concept.html', 18.35, 20.4] // end card
];
const DIP = 0.12;
const b = await chromium.launch();
const ctx = await b.newContext({ viewport: { width: 1080, height: 1080 }, deviceScaleFactor: 2 });
const pages = {};
const errs = [];
for (const name of new Set(CUTS.map((c) => c[0]))) {
	const p = await ctx.newPage();
	p.on('pageerror', (e) => errs.push(name + ': ' + e));
	await p.goto(pathToFileURL(new URL('./' + name, import.meta.url).pathname).href, { waitUntil: 'load' });
	await p.evaluate(() => { const d = document.createElement('div'); d.id = 'dip'; d.style.cssText = 'position:fixed;inset:0;background:#000;opacity:0;pointer-events:none;z-index:99'; document.body.appendChild(d); });
	pages[name] = p;
}
await new Promise((r) => setTimeout(r, 400));
const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', '30', '-c:v', 'png', '-i', '-', '-i', audio,
	'-vf', 'scale=1080:1080:flags=lanczos:out_color_matrix=bt709:out_range=tv,format=yuv420p',
	'-c:v', 'libx264', '-preset', 'veryslow', '-tune', 'animation', '-crf', crf, '-profile:v', 'high', '-level', '4.1',
	'-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-color_range', 'tv',
	'-c:a', 'aac', '-b:a', '128k', '-movflags', '+faststart', '-shortest', out], { stdio: ['pipe', 'inherit', 'inherit'] });
let total = 0;
for (const [ci, [name, a, z]] of CUTS.entries()) {
	const n = Math.round((z - a) * 30);
	const p = pages[name];
	await p.bringToFront();
	for (let i = 0; i < n; i++) {
		const t = i / 30; // time inside the cut
		const dip = ci > 0 ? Math.max(0, 1 - t / DIP) : 0, dipOut = ci < CUTS.length - 1 ? Math.max(0, 1 - (n / 30 - t) / DIP) : 0;
		await p.evaluate(([src, d]) => { window.render(src); document.getElementById('dip').style.opacity = d; }, [a + t, Math.max(dip, dipOut)]);
		const png = await p.screenshot({ type: 'png' });
		if (!ff.stdin.write(png)) await new Promise((r) => ff.stdin.once('drain', r));
		total++;
	}
}
ff.stdin.end();
await new Promise((r) => ff.on('close', r));
console.log('frames:', total, (total / 30).toFixed(2) + 's', errs.length ? 'ERRORS ' + errs.join(' | ') : 'ok');
await b.close();
