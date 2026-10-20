// A headless look at the fitting site: opens /sim/, imports a fit and screenshots the page.
//   node shot.mjs <base url> <out.png>   (playwright installed where node resolves it)
import { chromium } from 'playwright';

const [base, out] = [process.argv[2], process.argv[3] || 'sim.png'];
const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--use-gl=angle'] });
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
page.on('console', m => { if (m.type() === 'error') console.log('console:', m.text()); });
await page.goto(base + 'sim/', { waitUntil: 'networkidle' });
await page.waitForSelector('#loading', { state: 'detached', timeout: 60000 });
await page.waitForTimeout(3000);
await page.screenshot({ path: out });
console.log('shot', out);
await browser.close();
