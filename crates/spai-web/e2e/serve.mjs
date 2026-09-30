// Serves a trunk build as the site's nginx does: /wh/ is the app, any path under it that is no
// file gets index.html, and /api/ goes to the server.
//   node serve.mjs <dist> <port> <api base, e.g. http://127.0.0.1:8099>
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

const [dir, port, api] = process.argv.slice(2);
const root = path.resolve(dir);
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.gz': 'application/gzip' };

http.createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  if (url.pathname.startsWith('/api/') && api) {
    const up = http.request(api + url.pathname + url.search, { method: req.method, headers: req.headers }, r => {
      res.writeHead(r.statusCode, r.headers);
      r.pipe(res);
    });
    up.on('error', e => { res.writeHead(502); res.end(String(e)); });
    return req.pipe(up);
  }
  if (!url.pathname.startsWith('/wh/')) { res.writeHead(404); return res.end(); }
  let f = path.join(root, url.pathname.slice(4));
  if (!f.startsWith(root) || !fs.existsSync(f) || fs.statSync(f).isDirectory()) f = path.join(root, 'index.html');
  res.writeHead(200, { 'Content-Type': types[path.extname(f)] || 'application/octet-stream' });
  fs.createReadStream(f).pipe(res);
}).listen(+port, '127.0.0.1');
