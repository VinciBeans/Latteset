# workspace 拆分：latteset-core 与 src-tauri

调度器单测是全项目质量底线（ADR-0001），但若与 Tauri 同 crate，单测将被迫经过 tauri 编译与 webview 初始化。因此采用 Cargo workspace 两 crate：**latteset-core**（无 Tauri 依赖、无 IO 的纯逻辑：调度器、项目模型、日志解析、SyncTeX 解析、设置合并）与 **src-tauri**（仅接线：commands、notify、进程执行、事件发射）。依赖方向单向：core ← src-tauri。

- **状态**：已接受
- **备选方案**：单 crate 模块化（否决：核心逻辑测试耦合 tauri 全家桶；改 core 触发 tauri 依赖重编）
- **影响**：core 纪律——不得反向依赖 tauri；IO 边界经 trait 注入（CompileRunner、SyncTexProvider）
