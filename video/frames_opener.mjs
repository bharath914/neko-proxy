// Renders concept.html frame by frame: node frames.mjs <outDir> [t1,t2,...]
import { chromium } from 'playwright';
import { pathToFileURL } from 'node:url';
const [, , out, list] = process.argv;
const b = await chromium.launch();
const p = await (await b.newContext({ viewport: { width: 1080, height: 1080 }, deviceScaleFactor: 1 })).newPage();
const errs = []; p.on('pageerror', (e) => errs.push(String(e)));
await p.goto(pathToFileURL(new URL('./opener.html', import.meta.url).pathname).href, { waitUntil: 'load' });
await p.waitForTimeout(300);
const dur = await p.evaluate(() => window.DURATION);
const times = list ? list.split(',').map(Number) : Array.from({ length: Math.round(dur * 30) }, (_, i) => i / 30);
for (const [i, t] of times.entries()) {
  await p.evaluate((t) => window.render(t), t);
  await p.screenshot({ path: `${out}/${list ? 'k' + t : String(i).padStart(4, '0')}.png` });
}
console.log('frames:', times.length, errs.length ? 'ERRORS ' + errs.join(' | ') : 'ok');
await b.close();
