// RootFilePicker 组件单测（roadmap P0-②-1）：
// 锁住「多候选 → 让用户挑」「零候选 → 兜底列出全部 .tex」「选中的是**相对路径**」三条契约
// （相对路径是 update_settings({root_file}) 唯一接受的形态）。
import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import RootFilePicker from "../RootFilePicker.vue";

const ROOT = "E:/Works/tex-presso/test_file/projects/demo";

function mountPicker(props: Partial<InstanceType<typeof RootFilePicker>["$props"]> = {}) {
  return mount(RootFilePicker, {
    props: {
      root: ROOT,
      candidates: [],
      fallbackFiles: [],
      ...props,
    },
  });
}

describe("RootFilePicker", () => {
  it("多候选：列出全部候选，按相对路径排序", () => {
    const w = mountPicker({
      candidates: [`${ROOT}/z.tex`, `${ROOT}/chapters/a.tex`, `${ROOT}/b.tex`],
    });
    const rows = w.findAll(".file-row");
    expect(rows).toHaveLength(3);
    // 展示相对路径（不含项目根），排序用 localeCompare（`b.tex` 在 `chapters/` 之前）
    const texts = rows.map((r) => r.text());
    expect(texts[0]).toContain("b.tex");
    expect(texts[1]).toContain("chapters/a.tex");
    expect(texts[2]).toContain("z.tex");
    // 文案指向「多个可能」
    expect(w.find(".lead").text()).toContain("3");
  });

  it("多候选：点击行 emit select，且载荷是**项目内相对路径**", async () => {
    const w = mountPicker({ candidates: [`${ROOT}/chapters/intro.tex`] });
    await w.find(".file-row").trigger("click");
    expect(w.emitted("select")).toEqual([["chapters/intro.tex"]]);
  });

  it("零候选：退回兜底列表（项目内全部 .tex）", () => {
    const w = mountPicker({ fallbackFiles: [`${ROOT}/main.tex`, `${ROOT}/other.tex`] });
    expect(w.findAll(".file-row")).toHaveLength(2);
    expect(w.find(".lead").text()).toContain("没有找到");
  });

  it("零候选且无 .tex：给出空状态提示，不渲染列表", () => {
    const w = mountPicker();
    expect(w.findAll(".file-row")).toHaveLength(0);
    expect(w.find(".empty").exists()).toBe(true);
  });

  it("项目外路径被过滤（不提供不可用的候选）", () => {
    const w = mountPicker({ candidates: ["E:/Works/elsewhere/main.tex", `${ROOT}/ok.tex`] });
    const rows = w.findAll(".file-row");
    expect(rows).toHaveLength(1);
    expect(rows[0].text()).toContain("ok.tex");
  });

  it("中文候选路径正确展示与提交", async () => {
    const root = "E:/项目/中文测试工程";
    const w = mountPicker({ root, candidates: [`${root}/章节/第一章.tex`] });
    expect(w.find(".file-row").text()).toContain("第一章.tex");
    await w.find(".file-row").trigger("click");
    expect(w.emitted("select")).toEqual([["章节/第一章.tex"]]);
  });

  it("busy 时禁用点击（防重复提交）", () => {
    const w = mountPicker({ candidates: [`${ROOT}/main.tex`], busy: true });
    expect(w.find(".file-row").attributes("disabled")).toBeDefined();
  });

  it("error 透出给用户", () => {
    const w = mountPicker({ candidates: [`${ROOT}/main.tex`], error: "路径在项目外" });
    expect(w.find(".err").text()).toContain("路径在项目外");
  });

  it("关闭按钮 emit close", async () => {
    const w = mountPicker();
    await w.find(".head-close").trigger("click");
    expect(w.emitted("close")).toHaveLength(1);
  });
});
