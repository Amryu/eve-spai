// Serves a trunk build at /sim/ and the baked fitting data at /sde/ for a headless look:
//   node serve.mjs <dist> <port> <sde dir>
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

const [dist, port, sde] = [process.argv[2], Number(process.argv[3] || 6800), process.argv[4] || ''];
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.gz': 'application/gzip', '.json': 'application/json' };
http.createServer((req, res) => {
  const p = new URL(req.url, 'http://x').pathname;
  let file;
  if (p.startsWith('/sde/') && sde) file = path.join(sde, path.basename(p));
  else if (p.startsWith('/sim/')) {
    file = path.join(dist, p.slice('/sim/'.length));
    if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) file = path.join(dist, 'index.html');
  }
  if (!file || !fs.existsSync(file)) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
}).listen(port, '127.0.0.1', () => console.log(`serving ${dist} at http://127.0.0.1:${port}/sim/`));
