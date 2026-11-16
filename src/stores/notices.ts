// noticesStore（modules.md §9.2）：编译期的**运行时**提示（`compile-notice` 事件）。
//
// 为什么单独一个 store：这类提示不是"错误"（不进 error 列表、不改 phase），也不是设置里的
// 一次性通知（那些来自后端 `settings.notices`，点掉就写盘消失）。它只是"这一趟编译里发生了一件
// 用户该知道的事"（例如文档调用了外部 biber，但本机缺失或版本不匹配），生命周期就是"用户看到并点掉为止"：
// - **不随页面切换清空**（提示的产生与当前打开哪个文件无关）；
// - **不随编译成功清空**（那条提示可能解释的正是"为什么这次没有外部工具参与"）；
// - 只有用户点击才 `clear()`。
import { defineStore } from "pinia";
import { ref } from "vue";

export const useNoticesStore = defineStore("notices", () => {
  const runtime = ref<string[]>([]);

  /** 追加一条运行时提示；同文案去重（同一趟里重复检出不该在状态栏排成好几条）。 */
  function push(message: string) {
    const text = message.trim();
    if (!text || runtime.value.includes(text)) return;
    runtime.value.push(text);
  }

  /** 用户点掉：清空全部运行时提示（非持久，不写盘）。 */
  function clear() {
    runtime.value = [];
  }

  return { runtime, push, clear };
});
