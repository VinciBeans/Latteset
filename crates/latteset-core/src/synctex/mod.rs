//! SyncTeX（modules.md §5 / ADR-0008：接口抽象；实现见 provider 的 CLI 版与 [`parse`] 的**自解析**版）。

pub mod classify;
pub mod model;
pub mod parse;
pub mod provider;
pub mod resolve;

pub use classify::{classify_inverse_target, normalize_lexically, InverseTarget};
pub use model::{SourcePosition, SyncTexError, SyncTexPosition};
pub use parse::{NodeKind, SyncTexDoc, UNIT_FACTOR};
pub use provider::{parse_forward_output, parse_inverse_output, SyncTexProvider};
pub use resolve::{
    pdf_path_for_root, resolve_inverse, synctex_data_path, InverseResolution,
    FALLBACK_Y_OFFSETS,
};
