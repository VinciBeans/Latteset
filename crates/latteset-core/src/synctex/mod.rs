//! SyncTeX（modules.md §5 / ADR-0008：走 synctex CLI + 接口抽象）。

pub mod classify;
pub mod model;
pub mod provider;
pub mod resolve;

pub use classify::{classify_inverse_target, normalize_lexically, InverseTarget};
pub use model::{SourcePosition, SyncTexError, SyncTexPosition};
pub use provider::{parse_forward_output, parse_inverse_output, SyncTexProvider};
pub use resolve::{
    pdf_path_for_root, resolve_inverse, synctex_data_path, InverseResolution,
    FALLBACK_Y_OFFSETS,
};
