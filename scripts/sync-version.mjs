// Đồng bộ phiên bản: chỉ cần sửa "version" trong version.json (hoặc truyền tham số), rồi chạy
//   node scripts/sync-version.mjs            -> đọc version.json và ghi vào 5 file bên dưới
//   node scripts/sync-version.mjs 0.1.8      -> ghi 0.1.8 vào version.json rồi đồng bộ luôn
// File được cập nhật: package.json, package-lock.json, src-tauri/tauri.conf.json,
//                     src-tauri/Cargo.toml, src-tauri/Cargo.lock (mục của chính app)
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const P = (...p) => path.join(root, ...p);
const VERSION_FILE = P('version.json');
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

const arg = process.argv[2]?.replace(/^v/i, '');
if (arg) {
  if (!SEMVER.test(arg)) fail(`Phiên bản "${arg}" không hợp lệ (ví dụ đúng: 0.1.8).`);
  fs.writeFileSync(VERSION_FILE, JSON.stringify({ version: arg }, null, 2) + '\n');
}

if (!fs.existsSync(VERSION_FILE)) fail('Không thấy version.json ở thư mục gốc.');
const version = JSON.parse(fs.readFileSync(VERSION_FILE, 'utf8')).version;
if (!SEMVER.test(version)) fail(`version.json có phiên bản không hợp lệ: "${version}"`);

const changed = [];
const note = (file, ok) => changed.push(`${ok ? '✔' : '·'} ${file}${ok ? '' : ' (đã đúng / không có)'}`);

/** Sửa file JSON, giữ nguyên kiểu thụt lề và xuống dòng cuối file. */
function editJson(rel, mutate) {
  const file = P(rel);
  if (!fs.existsSync(file)) return note(rel + ' — không tìm thấy', false);
  const raw = fs.readFileSync(file, 'utf8');
  const indent = raw.match(/^[ \t]+(?=")/m)?.[0] ?? '  ';
  const data = JSON.parse(raw);
  const before = JSON.stringify(data);
  mutate(data);
  const eol = raw.includes('\r\n') ? '\r\n' : '\n';
  const out = JSON.stringify(data, null, indent).replace(/\n/g, eol) + (raw.endsWith('\n') ? eol : '');
  if (JSON.stringify(data) !== before) fs.writeFileSync(file, out);
  note(rel, JSON.stringify(data) !== before);
}

editJson('package.json', (d) => { d.version = version; });
editJson('package-lock.json', (d) => {
  d.version = version;
  if (d.packages?.['']) d.packages[''].version = version;
});
editJson(path.join('src-tauri', 'tauri.conf.json'), (d) => { d.version = version; });

// Cargo.toml: chỉ đổi dòng version nằm trong [package], không đụng version của thư viện phụ thuộc.
{
  const rel = path.join('src-tauri', 'Cargo.toml');
  const file = P(rel);
  const raw = fs.readFileSync(file, 'utf8');
  let inPackage = false, done = false;
  const out = raw.split(/(\r?\n)/).map((line) => {
    if (/^\s*\[.*\]\s*$/.test(line)) inPackage = /^\s*\[package\]\s*$/.test(line);
    if (inPackage && !done && /^\s*version\s*=/.test(line)) {
      done = true;
      return line.replace(/"[^"]*"/, `"${version}"`);
    }
    return line;
  }).join('');
  if (!done) fail('Không tìm thấy dòng version trong [package] của Cargo.toml.');
  if (out !== raw) fs.writeFileSync(file, out);
  note(rel, out !== raw);
}

// Cargo.lock: cargo tự cập nhật khi build, nhưng sửa sẵn để `git diff` sạch và build với --locked không lỗi.
{
  const rel = path.join('src-tauri', 'Cargo.lock');
  const file = P(rel);
  if (fs.existsSync(file)) {
    const name = fs.readFileSync(P('src-tauri', 'Cargo.toml'), 'utf8').match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1];
    const raw = fs.readFileSync(file, 'utf8');
    const re = new RegExp(`(\\[\\[package\\]\\]\\r?\\nname = "${name}"\\r?\\nversion = )"[^"]*"`);
    const out = raw.replace(re, `$1"${version}"`);
    if (out !== raw) fs.writeFileSync(file, out);
    note(rel, out !== raw);
  }
}

console.log(`Phiên bản: ${version}\n` + changed.join('\n'));

function fail(msg) { console.error('Lỗi: ' + msg); process.exit(1); }
