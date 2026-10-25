// Renders concept.html at 2x and pipes the frames into ffmpeg (x264, downscaled to 1080 with lanczos).
// node render.mjs <audio.wav> <out.mp4> [crf]
import { chromium } from 'playwright';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';
const [, , audio, out, crf = '20', page = 'concept.html'] = process.argv;
const b = await chromium.launch();
const p = await (await b.newContext({ viewport: { width: 1080, height: 1080 }, deviceScaleFactor: 2 })).newPage();
const errs = []; p.on('pageerror', (e) => errs.push(String(e)));
await p.goto(pathToFileURL(new URL('./' + page, import.meta.url).pathname).href, { waitUntil: 'load' });
await p.waitForTimeout(300);
const dur = await p.evaluate(() => window.DURATION);
const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', '30', '-c:v', 'png', '-i', '-', '-i', audio,
	'-vf', 'scale=1080:1080:flags=lanczos:out_color_matrix=bt709:out_range=tv,format=yuv420p',
	'-c:v', 'libx264', '-preset', 'veryslow', '-tune', 'animation', '-crf', crf, '-profile:v', 'high', '-level', '4.1',
	'-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-color_range', 'tv',
	'-c:a', 'aac', '-b:a', '128k', '-movflags', '+faststart', '-shortest', out], { stdio: ['pipe', 'inherit', 'inherit'] });
const frames = Math.round(dur * 30);
for (let i = 0; i < frames; i++) {
	await p.evaluate((t) => window.render(t), i / 30);
	const png = await p.screenshot({ type: 'png' });
	if (!ff.stdin.write(png)) await new Promise((r) => ff.stdin.once('drain', r));
}
ff.stdin.end();
await new Promise((r) => ff.on('close', r));
console.log('frames:', frames, errs.length ? 'ERRORS ' + errs.join(' | ') : 'ok');
await b.close();
