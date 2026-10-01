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
await page.evaluate(s => { localStorage.setItem('spai.session', s); localStorage.setItem('spai.invite.here', 'true'); }, session);
// SPAI_E2E_ESI: a character added with every scope, and ESI answered here: in Jita, JDC 5, JFC 4.
const waypoints = [];
let locReads = 0;
if (process.env.SPAI_E2E_ESI) {
  const acc = [{ char_id: 90000777, name: 'Alt Pilot', scopes: ['esi-location.read_location.v1', 'esi-location.read_online.v1', 'esi-ui.write_waypoint.v1', 'esi-skills.read_skills.v1'],
    access: 'x', refresh: 'y', expires_at: 4102444800 }];
  await page.evaluate(a => localStorage.setItem('spai.accounts', a), JSON.stringify(acc));
  await page.route('https://esi.evetech.net/**', r => {
    const u = r.request().url();
    const json = b => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(b), headers: { 'access-control-allow-origin': '*' } });
    // SPAI_E2E_ESI_JUMP: in Jita for two reads, then in that system, as if through a hole.
    if (u.includes('/location/')) { locReads++; const to = Number(process.env.SPAI_E2E_ESI_JUMP || 0); return json({ solar_system_id: to && locReads > 2 ? to : 30000142 }); }
    if (u.includes('/online/')) return json({ online: true });
    if (u.includes('/skills/')) return json({ skills: [{ skill_id: 21611, trained_skill_level: 5 }, { skill_id: 21610, trained_skill_level: 4 }] });
    if (u.includes('/waypoint/')) { waypoints.push(new URL(u).searchParams.get('destination_id')); return r.fulfill({ status: 204, headers: { 'access-control-allow-origin': '*' } }); }
    return r.fulfill({ status: 404 });
  });
}
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
if (mode === 'admin') {
  // An admin now: the Group tab, where the next request waits, then Approve on it.
  const at = JSON.parse(process.env.SPAI_E2E_CLICKS || '[]');
  for (const [i, [x, y]] of at.entries()) {
    // Requests show after the round that fetches them.
    await page.waitForTimeout(i === 0 ? 1000 : 17000);
    await page.mouse.click(x, y);
    await page.waitForTimeout(800);
    await page.screenshot({ path: shot.replace('.png', `-admin${i}.png`) });
  }
  // The approval goes out on the round it starts.
  await page.waitForTimeout(6000);
  console.log('approved-there');
}
if (process.env.SPAI_E2E_MAP) {
  const [x, y] = JSON.parse(process.env.SPAI_E2E_MAP);
  await page.mouse.click(x, y);
  await page.waitForTimeout(1500);
  await page.screenshot({ path: shot.replace('.png', '-map.png') });
}
// Then any steps a test gives: ["click", x, y], ["rclick", x, y], ["drag", x1, y1, x2, y2],
// ["move", x, y], ["wheel", x, y, dy], ["sdrag", x1, y1, x2, y2] (with shift), ["key", "Enter"], ["type", "text"], ["wait", ms] or ["shot", "name"].
for (const step of JSON.parse(process.env.SPAI_E2E_ACTIONS || '[]')) {
  const [what, ...a] = step;
  if (what === 'click') await page.mouse.click(a[0], a[1]);
  if (what === 'rclick') await page.mouse.click(a[0], a[1], { button: 'right' });
  if (what === 'sdrag') {
    await page.keyboard.down('Shift');
    await page.mouse.move(a[0], a[1]);
    await page.mouse.down();
    for (let k = 1; k <= 12; k++) await page.mouse.move(a[0] + (a[2] - a[0]) * k / 12, a[1] + (a[3] - a[1]) * k / 12);
    await page.waitForTimeout(300);
    await page.mouse.up();
    await page.keyboard.up('Shift');
  }
  if (what === 'drag') {
    await page.mouse.move(a[0], a[1]);
    await page.mouse.down();
    for (let k = 1; k <= 12; k++) await page.mouse.move(a[0] + (a[2] - a[0]) * k / 12, a[1] + (a[3] - a[1]) * k / 12);
    await page.waitForTimeout(300);
    await page.mouse.up();
  }
  if (what === 'wheel') { await page.mouse.move(a[0], a[1]); await page.mouse.wheel(0, a[2]); }
  if (what === 'move') await page.mouse.move(a[0], a[1], { steps: 5 });
  if (what === 'key') await page.keyboard.press(a[0]);
  if (what === 'type') await page.keyboard.type(a[0], { delay: 30 });
  if (what === 'wait') await page.waitForTimeout(a[0]);
  if (what === 'shot') await page.screenshot({ path: shot.replace('.png', `-${a[0]}.png`) });
  await page.waitForTimeout(500);
}
if (process.env.SPAI_E2E_ESI) console.log('waypoints', JSON.stringify(waypoints));
const s = JSON.parse(await page.evaluate(() => localStorage.getItem('spai.store')) || '{}');
console.log('holds', JSON.stringify({ groups: (s.groups || []).map(g => [g.name, g.role, g.epoch]), keys: (s.keys || []).length, holes: Object.values(s.holes || {}).map(h => h.state.fields.signature?.v), sigs: Object.values(s.sigs || {}).flat().map(x => x.sig) }));
await browser.close();
process.exit(0);
