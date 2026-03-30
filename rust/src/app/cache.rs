use super::*;

#[derive(Default)]
pub(super) struct PreviewCacheState {
    pub(super) entries: HashMap<PathBuf, String>,
    pub(super) order: VecDeque<PathBuf>,
    pub(super) total_bytes: usize,
}

#[derive(Default)]
pub(super) struct HighlightCacheState {
    pub(super) scope_query: String,
    pub(super) scope_root: PathBuf,
    pub(super) scope_use_regex: bool,
    pub(super) scope_ignore_case: bool,
    pub(super) scope_prefer_relative: bool,
    pub(super) entries: HashMap<HighlightCacheKey, Arc<Vec<u16>>>,
    pub(super) order: VecDeque<HighlightCacheKey>,
}

#[derive(Default)]
pub(super) struct SortMetadataCacheState {
    pub(super) entries: HashMap<PathBuf, SortMetadata>,
    pub(super) order: VecDeque<PathBuf>,
}
