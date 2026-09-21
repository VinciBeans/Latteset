// 引擎清单（roadmap ㊻）：**唯一来源是后端** `list_engines`（core `engine::engine_list`）。
//
// 为什么不在前端写一份数组：顺序、名字、说明与"本机能不能用"是同一件事的四个面，抄一份就会飘
// ——而且"本机装没装 TeX Live"只有后端知道（PATH 探测在 infra）。
//
// 缓存策略：整个会话取一次。清单只在两种情况下会变——装了/卸了引擎、改了 `LATTESET_TEX_ENGINES`
// ——两者都要求重启应用（PATH 是进程启动时读的），所以"取一次"与"每次取"没有可观测差别。
import { ipc } from "./ipc";
import type { EngineInfo } from "../bindings";

let cached: Promise<EngineInfo[]> | null = null;

/** 拉清单（失败会抛出，调用方各自决定怎么退化）。 */
export function loadEngines(): Promise<EngineInfo[]> {
  cached ??= ipc.listEngines().catch((e) => {
    cached = null; // 失败不缓存：下次打开面板再试（网络式抖动不存在，但别把一次失败钉死）
    throw e;
  });
  return cached;
}

/** 引擎 id → 显示名；清单还没到手时退化成 id 本身（状态栏不能因为一次 IPC 失败就空着）。 */
export function labelOf(engines: EngineInfo[], id: string): string {
  return engines.find((e) => e.id === id)?.label ?? id;
}

/**
 * **测试用**：清掉会话缓存。
 *
 * 真实运行不需要它——清单只在"装了/卸了引擎、改了 `LATTESET_TEX_ENGINES`"之后才会变，而那两件事
 * 都要求重启应用（PATH 是进程启动时读的）。单测要在同一个进程里换几套清单（可用/不可用/被收窄），
 * 所以留一个显式的清缓存口子，而不是把缓存去掉。
 */
export function __resetEngineCatalogForTest() {
  cached = null;
}
