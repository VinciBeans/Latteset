#!/usr/bin/env node
// 把 Markdown 里的 Mermaid 代码块渲染成独立 SVG（无头 Edge/Chrome，离线，不联网）。
//
// 用法：
//   node scripts/render-diagrams.mjs [md 文件] [输出目录] [--png]
//   默认：docs/architecture-diagram.md -> docs/diagrams
//
// mermaid 解析顺序：$MERMAID_DIST > ./node_modules/mermaid/dist/mermaid.min.js
// 本仓库未把 mermaid 列为依赖（仅文档渲染用），需要时临时安装：npm i mermaid@12。
// 浏览器解析顺序：$CHROME_PATH > 常见 Edge / Chrome 安装路径。
//
// 图名：代码块前一行写 <!-- mermaid: some-name --> 可指定输出文件名，否则 figure-N。
//
// 实现说明：浏览器输出经 **文件重定向**（cmd/sh 的 >）落盘，不用管道捕获 stdout——
// 受限沙箱下 Node 的 piped stdio 会 EPERM，重定向则无管道。

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

const BT = String.fromCharCode(96);
const FENCE = BT + BT + BT;
const IS_WIN = process.platform === 'win32';

const argv = process.argv.slice(2);
const wantPng = argv.includes('--png');
const positional = argv.filter((a) => !a.startsWith('--'));
const mdPath = resolve(positional[0] ?? 'docs/architecture-diagram.md');
const outDir = resolve(positional[1] ?? 'docs/diagrams');
const rel = (p) => (p.startsWith(process.cwd() + sep) ? p.slice(process.cwd().length + 1) : p);

function findMermaid() {
  const candidates = [
    process.env.MERMAID_DIST,
    join(process.cwd(), 'node_modules/mermaid/dist/mermaid.min.js'),
  ].filter(Boolean);
  for (const c of candidates) if (existsSync(c)) return resolve(c);
  console.error('找不到 mermaid 打包产物：npm i mermaid@12，或用 MERMAID_DIST 指向 mermaid.min.js');
  process.exit(1);
}

function findBrowser() {
  const candidates = [
    process.env.CHROME_PATH,
    'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
    'C:/Program Files/Google/Chrome/Application/chrome.exe',
    'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
  ].filter(Boolean);
  for (const c of candidates) if (existsSync(c)) return c;
  console.error('找不到 Edge/Chrome：用 CHROME_PATH 指定可执行文件');
  process.exit(1);
}

function q(s) {
  return '"' + String(s).replace(/"/g, '') + '"';
}

/** 跑一条命令行（走 shell，stdio 全部 NUL/继承外：无管道）。 */
function runLine(line) {
  const r = spawnSync(line, { shell: true, stdio: 'ignore' });
  if (r.error) throw r.error;
  return r.status ?? 0;
}

function escapeHtml(s) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function harnessPage(code, mermaidDist, title) {
  const cfg = [
    'mermaid.initialize({',
    'startOnLoad:false,',
    'securityLevel:"loose",',
    'theme:"default",',
    'themeVariables:{fontFamily:\'"Segoe UI","Microsoft YaHei",sans-serif\',fontSize:"13px"},',
    'flowchart:{htmlLabels:true,useMaxWidth:false},',
    'sequence:{useMaxWidth:false},',
    'state:{useMaxWidth:false}',
    '});',
  ].join('');
  const js = [
    '(async function(){',
    'var s=document.getElementById("status");',
    'try{',
    cfg,
    'await mermaid.run({nodes:[document.getElementById("d")]});',
    's.textContent="OK";',
    '}catch(e){s.textContent="ERROR: "+(e&&e.message?e.message:String(e));}',
    '})();',
  ].join('');
  return [
    '<!doctype html><html lang="zh"><head><meta charset="utf-8"><title>' + title + '</title>',
    '<style>body{margin:0;padding:10px;background:#fff}',
    'pre{font:12px/1.5 Consolas,monospace;color:#4a4cd8}</style>',
    '<script src="' + pathToFileURL(mermaidDist).href + '"></' + 'script>',
    '</head><body>',
    '<div class="mermaid" id="d">' + escapeHtml(code) + '</div>',
    '<pre id="status">PENDING</pre>',
    '<script>' + js + '</' + 'script>',
    '</body></html>',
  ].join('\n');
}

/** 从 dump 出来的 DOM 取状态与 SVG；SVG 注入白底（独立查看时背景不透明）。 */
function extract(dom) {
  const st = /<pre id="status">([\s\S]*?)<\/pre>/.exec(dom);
  const status = st ? st[1].trim() : '(未取到状态)';
  const i = dom.indexOf('<svg');
  const j = dom.lastIndexOf('</svg>');
  if (i < 0 || j < 0) return { status, svg: null };
  const svg = dom.slice(i, j + 6).replace('>', '><rect width="100%" height="100%" fill="#ffffff"/>');
  return { status, svg };
}

function svgSize(svg) {
  const tag = svg.slice(0, svg.indexOf('>') + 1);
  const num = (re, fallback) => {
    const m = re.exec(tag);
    return m ? Math.ceil(Number(m[1])) : fallback;
  };
  const box = /viewBox="0 0 ([0-9.]+) ([0-9.]+)"/.exec(tag);
  return {
    w: num(/width="([0-9.]+)"/, box ? Math.ceil(Number(box[1])) : 1600),
    h: num(/height="([0-9.]+)"/, box ? Math.ceil(Number(box[2])) : 1200),
  };
}

const md = readFileSync(mdPath, 'utf8');
const re = new RegExp('^' + FENCE + 'mermaid\\r?\\n([\\s\\S]*?)^' + FENCE, 'gm');
const blocks = [];
let m;
while ((m = re.exec(md)) !== null) {
  const named = /<!--\s*mermaid:\s*([A-Za-z0-9._-]+)\s*-->\s*$/.exec(md.slice(0, m.index));
  blocks.push({ name: named ? named[1] : 'figure-' + (blocks.length + 1), code: m[1] });
}
if (blocks.length === 0) {
  console.error('未在 ' + mdPath + ' 找到 mermaid 代码块');
  process.exit(1);
}

const mermaidDist = findMermaid();
const browser = findBrowser();
const workDir = mkdtempSync(join(tmpdir(), 'txp-diagram-'));
mkdirSync(outDir, { recursive: true });

const COMMON = [
  '--headless=new',
  '--disable-gpu',
  '--no-first-run',
  '--disable-extensions',
  '--hide-scrollbars',
  '--user-data-dir=' + q(join(workDir, 'profile')),
  '--virtual-time-budget=10000',
].join(' ');

let failed = 0;
console.log('mermaid: ' + mermaidDist);
console.log('browser: ' + browser);
for (const b of blocks) {
  const htmlPath = join(workDir, b.name + '.html');
  const dumpPath = join(workDir, b.name + '.dump.html');
  writeFileSync(htmlPath, harnessPage(b.code, mermaidDist, b.name), 'utf8');
  const url = pathToFileURL(htmlPath).href;

  const code = runLine(q(browser) + ' ' + COMMON + ' --dump-dom ' + q(url) + ' > ' + q(dumpPath));
  const dom = existsSync(dumpPath) ? readFileSync(dumpPath, 'utf8') : '';
  const { status, svg } = code === 0 ? extract(dom) : { status: '浏览器退出码 ' + code, svg: null };
  if (!svg || status !== 'OK') {
    failed++;
    console.error('x ' + b.name + ' -> ' + status);
    continue;
  }

  const svgPath = join(outDir, b.name + '.svg');
  writeFileSync(svgPath, '<?xml version="1.0" encoding="UTF-8"?>\n' + svg + '\n', 'utf8');
  let extra = '';
  if (wantPng) {
    const { w, h } = svgSize(svg);
    const pngPath = join(outDir, b.name + '.png');
    const shot = runLine(
      q(browser) + ' ' + COMMON +
      ' --force-device-scale-factor=1' +
      ' --window-size=' + Math.min(w + 40, 8000) + ',' + Math.min(h + 60, 8000) +
      ' --screenshot=' + q(pngPath) + ' ' + q(url),
    );
    if (shot === 0) extra = ', ' + rel(pngPath);
    else { failed++; console.error('x ' + b.name + ' 截图失败（退出码 ' + shot + '）'); }
  }
  console.log('OK ' + b.name + ' -> ' + rel(svgPath) + extra);
}
console.log(failed === 0 ? '全部通过（' + blocks.length + ' 图）' : failed + ' 项失败');
process.exit(failed === 0 ? 0 : 1);
