#!/usr/bin/env node
// PDF 产物校验（方案 §6 的 P2 复核命令；`docs/tectonic-library-plan.md:518`）。
//
// 存在理由：`compile` 报 success 只说明引擎没报错，**不等于产物可用**。本脚本独立把 PDF 读回来，
// 验四件事——① 是不是真 PDF（magic + 可解析）；② 页数对不对；③ 文本层抽得出来、内容对得上
// （`--expect-text`，中文文档用它验 CJK 字体确实嵌进去了）；④ 文本层里没有替换字符（U+FFFD）。
//
// 用法：
//   node scripts/validate-pdf.mjs --pdf <path.pdf> [--expect-pages N]
//        [--expect-text <子串>]... [--min-text-chars N] [--json]
//
// 退出码：0 = 全部通过；1 = 校验失败（是结论）；2 = 用法/环境错误（不是结论）。
//
// 依赖 `pdfjs-dist`（已在 dependencies 里，草案层同源：`src/services/draftPatch.ts` 的文本层
// 口径与这里一致）。口径与 `test_file/projects/bench/_e1/e2-textlayer.mjs` 相同：Node + legacy
// 构建、无 worker / 无 DOM / 无 canvas ⇒ 只测「文本层」，不渲染位图。

import { existsSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";

const USAGE = `用法：node scripts/validate-pdf.mjs --pdf <path.pdf> [--expect-pages N]
      [--expect-text <子串>]... [--min-text-chars N] [--json]`;

function parseArgs(argv) {
  const out = { pdf: null, expectPages: null, expectText: [], minTextChars: 0, json: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--pdf") out.pdf = argv[++i];
    else if (a === "--expect-pages") out.expectPages = Number(argv[++i]);
    else if (a === "--expect-text") out.expectText.push(argv[++i]);
    else if (a === "--min-text-chars") out.minTextChars = Number(argv[++i]);
    else if (a === "--json") out.json = true;
    else if (a === "-h" || a === "--help") out.help = true;
    else throw new Error(`未知参数：${a}`);
  }
  return out;
}

/** 每条检查：`{name, ok, detail}`；全 ok 才算通过。 */
function check(name, ok, detail) {
  return { name, ok: Boolean(ok), detail };
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    console.error(USAGE);
    process.exit(2);
  }
  if (args.help) {
    console.log(USAGE);
    return;
  }
  if (!args.pdf) {
    console.error("缺少 --pdf <path.pdf>");
    console.error(USAGE);
    process.exit(2);
  }

  const pdfPath = resolve(args.pdf);
  const checks = [];

  if (!existsSync(pdfPath)) {
    console.error(`PDF 不存在：${pdfPath}`);
    process.exit(1);
  }
  const size = statSync(pdfPath).size;
  const buf = readFileSync(pdfPath);
  checks.push(check("非空", size > 0, `${size} B`));
  // magic：PDF 头允许前面有少量垃圾（规范允许偏移，但我们自己产出的必须是 0 偏移）
  const head = buf.subarray(0, 8).toString("latin1");
  checks.push(check("%PDF- 头", head.startsWith("%PDF-"), JSON.stringify(head)));
  checks.push(check("EOF 标记", buf.subarray(-1024).toString("latin1").includes("%%EOF"), "尾部 1 KiB 内"));

  // 解析 + 页数 + 逐页文本
  let doc = null;
  let task = null;
  let pages = [];
  try {
    task = getDocument({
      data: new Uint8Array(buf),
      disableFontFace: true,
      useSystemFonts: false,
      // CJK 必须给 cmaps，否则字体翻译失败（`Ensure that the cMapUrl API parameter is provided`）
      // 会只抽出部分文本 —— 中文文档的校验就失去意义。
      cMapUrl: resolve("node_modules/pdfjs-dist/cmaps") + "/",
      cMapPacked: true,
      standardFontDataUrl: resolve("node_modules/pdfjs-dist/standard_fonts") + "/",
    });
    doc = await task.promise;
    checks.push(check("pdf.js 可解析", true, `pdfjs numPages=${doc.numPages}`));
    for (let i = 1; i <= doc.numPages; i++) {
      const page = await doc.getPage(i);
      const content = await page.getTextContent();
      pages.push(content.items.map((it) => it.str ?? "").join(""));
    }
  } catch (e) {
    checks.push(check("pdf.js 可解析", false, String(e && e.message ? e.message : e)));
  } finally {
    if (task) await task.destroy();
  }

  if (args.expectPages !== null) {
    checks.push(
      check("页数一致", pages.length === args.expectPages, `期望 ${args.expectPages}，实测 ${pages.length}`),
    );
  }

  const text = pages.join("\n");
  const stripped = text.replace(/\s+/g, "");
  checks.push(check("文本层非空", stripped.length > 0, `${stripped.length} 个非空白字符`));
  if (args.minTextChars > 0) {
    checks.push(
      check(
        `文本层 ≥ ${args.minTextChars} 字`,
        stripped.length >= args.minTextChars,
        `实测 ${stripped.length}`,
      ),
    );
  }
  for (const needle of args.expectText) {
    checks.push(check(`包含 ${JSON.stringify(needle)}`, text.includes(needle), text.includes(needle) ? "" : "未找到"));
  }
  const bad = (text.match(/\uFFFD/g) || []).length;
  checks.push(check("无 U+FFFD", bad === 0, `${bad} 个替换字符`));

  const ok = checks.every((c) => c.ok);
  const report = {
    ok,
    pdf: pdfPath,
    bytes: size,
    pages: pages.length,
    textChars: stripped.length,
    perPageChars: pages.map((p) => p.replace(/\s+/g, "").length),
    checks,
  };

  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(`PDF：${pdfPath}`);
    console.log(`  ${size} B / ${pages.length} 页 / 文本层 ${stripped.length} 字`);
    for (const c of checks) {
      console.log(`  [${c.ok ? "PASS" : "FAIL"}] ${c.name}${c.detail ? ` — ${c.detail}` : ""}`);
    }
    console.log(ok ? "校验通过" : "校验未通过");
  }
  process.exit(ok ? 0 : 1);
}

main().catch((e) => {
  console.error(`validate-pdf 内部错误：${e && e.stack ? e.stack : e}`);
  process.exit(2);
});
