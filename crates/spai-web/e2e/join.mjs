// The web app joining a group, driven by the desktop engine's `web_joins_through_an_invite` test.
//   node join.mjs <site> <invite path> <session json> <shot.png>
// Prints `requested` once the join is sent, then waits for `approved` on stdin, then prints what
// the browser holds. WebGL is software only (SwiftShader), never the desktop GPU.
import { chromium } from 'playwright';
import readline from 'node:readline';

const [site, invite, session, shot, mode] = process.argv.slice(2);
const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--use-gl=angle'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
page.on('pageerror', e => console.log('pageerror', e.message));
const ready = () => page.waitForFunction(() => !document.getElementById('loading'), null, { timeout: 30000 });

// Signed in without EVE: the test minted the session.
await page.goto(site + '/wh/');
await page.evaluate(s => localStorage.setItem('spai.session', s), session);
await page.goto(site + invite);
await ready();
// Poll until the join is in the saved store: the group appears there once the engine sent it.
for (let i = 0; i < 60; i++) {
  const s = await page.evaluate(() => localStorage.getItem('spai.store'));
  if (s && JSON.parse(s).groups.length) break;
  await page.waitForTimeout(1000);
}
await page.screenshot({ path: shot.replace('.png', '-waiting.png') });
console.log('requested');

const rl = readline.createInterface({ input: process.stdin });
for await (const line of rl) if (line.trim() === 'approved') break;
// An open stdin would keep node running, and the test waits for it to end.
rl.close();
process.stdin.destroy();

// A round is due every 15 s; give it two.
for (let i = 0; i < 40; i++) {
  const s = JSON.parse(await page.evaluate(() => localStorage.getItem('spai.store')) || '{}');
  if (s.holes && Object.keys(s.holes).length) break;
  await page.waitForTimeout(1000);
}
await page.waitForTimeout(1500);
await page.screenshot({ path: shot });
// Where the lone J-space system lands in this layout: selected, for a look at the side panel.
await page.mouse.click(905, 527);
await page.waitForTimeout(800);
await page.screenshot({ path: shot.replace('.png', '-selected.png') });
if (mode === 'edit') {
  // The lone hole's pencil, then Critical in the Mass row, then Save: positions in this layout.
  const at = JSON.parse(process.env.SPAI_E2E_CLICKS || '[]');
  for (const [i, [x, y]] of at.entries()) {
    await page.mouse.click(x, y);
    await page.waitForTimeout(600);
    await page.screenshot({ path: shot.replace('.png', `-edit${i}.png`) });
  }
  // The edit goes out on the round it starts.
  for (let i = 0; i < 30; i++) {
    const s = JSON.parse(await page.evaluate(() => localStorage.getItem('spai.store')) || '{}');
    if ((s.outbox || []).length === 0 && Object.values(s.holes || {}).some(h => h.state.fields.mass)) break;
    await page.waitForTimeout(1000);
  }
  console.log('edited');
}
const s = JSON.parse(await page.evaluate(() => localStorage.getItem('spai.store')) || '{}');
console.log('holds', JSON.stringify({ groups: (s.groups || []).map(g => [g.name, g.role, g.epoch]), keys: (s.keys || []).length, holes: Object.values(s.holes || {}).map(h => h.state.fields.signature?.v), sigs: Object.values(s.sigs || {}).flat().map(x => x.sig) }));
await browser.close();
process.exit(0);
