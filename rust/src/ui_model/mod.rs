mod display;
mod highlight;
mod on_demand;
mod paged_preview;
mod preview;
mod syntax_preview;

pub use paged_preview::{PagedTextPreview, PreviewPageError, PreviewPageState};
pub use syntax_preview::{SyntaxHighlight, SyntaxLanguage, SyntaxSpan, SyntaxTokenKind};

pub use display::{display_path, display_path_with_mode, normalize_path_for_display};
pub use highlight::{
    has_visible_match, match_positions_for_path, match_positions_for_path_with_compiled,
};
pub use on_demand::should_skip_preview;
pub use preview::{
    build_preview_text, build_preview_text_with_kind, build_preview_text_with_kind_cancellable,
};
pub(crate) use preview::{format_file_size, format_system_time, metadata_attributes};
