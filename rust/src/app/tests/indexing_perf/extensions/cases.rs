use super::fixture::Shape;
use super::oracle::Filter;
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Profile {
    Files,
    Folders,
    Ignore,
    IgnoreCase,
    MidIgnore,
    SearchIgnore,
    EditFiles,
    NameShown,
    ModifiedShown,
    NameAll,
    ModifiedAll,
    Warm,
    Promotion,
    TabChain,
    StableSelective,
    StableDense,
    StableEdit,
    Reclaim,
    WarmReclaim,
    NestedEarly,
    NestedLate,
    Preview,
    Links,
    Deep,
    Wide,
    Truncated,
    ParserFiles,
    ParserFolders,
}
impl Profile {
    pub(super) const ALL: [Self; 28] = [
        Self::Files,
        Self::Folders,
        Self::Ignore,
        Self::IgnoreCase,
        Self::MidIgnore,
        Self::SearchIgnore,
        Self::EditFiles,
        Self::NameShown,
        Self::ModifiedShown,
        Self::NameAll,
        Self::ModifiedAll,
        Self::Warm,
        Self::Promotion,
        Self::TabChain,
        Self::StableSelective,
        Self::StableDense,
        Self::StableEdit,
        Self::Reclaim,
        Self::WarmReclaim,
        Self::NestedEarly,
        Self::NestedLate,
        Self::Preview,
        Self::Links,
        Self::Deep,
        Self::Wide,
        Self::Truncated,
        Self::ParserFiles,
        Self::ParserFolders,
    ];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Files => "F1-files",
            Self::Folders => "F1-folders",
            Self::Ignore => "F1-ignore-list",
            Self::IgnoreCase => "F1-ignore-case",
            Self::MidIgnore => "F1-mid-ignore",
            Self::SearchIgnore => "S1-ignore",
            Self::EditFiles => "S2-files",
            Self::NameShown => "O1-name-shown",
            Self::ModifiedShown => "O1-modified-shown",
            Self::NameAll => "O1-name-all",
            Self::ModifiedAll => "O1-modified-all",
            Self::Warm => "T1-active-warm",
            Self::Promotion => "T1-promotion",
            Self::TabChain => "T1-A-B-C-A",
            Self::StableSelective => "T1-S1-selective",
            Self::StableDense => "T1-S1-dense",
            Self::StableEdit => "T1-S2",
            Self::Reclaim => "R1-natural",
            Self::WarmReclaim => "T1-natural-reclaim",
            Self::NestedEarly => "H1-early",
            Self::NestedLate => "H1-late",
            Self::Preview => "P1-preview",
            Self::Links => "W1-follow-links",
            Self::Deep => "W1-deep",
            Self::Wide => "W1-wide",
            Self::Truncated => "W1-truncated",
            Self::ParserFiles => "F1-filelist-parser-files",
            Self::ParserFolders => "F1-filelist-parser-folders",
        }
    }
    pub(super) fn parse(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|p| p.name() == name)
            .unwrap_or_else(|| panic!("unknown extension profile {name}"))
    }
    pub(super) fn sources(self) -> Vec<Source> {
        match self {
            Self::Files
            | Self::Folders
            | Self::EditFiles
            | Self::Links
            | Self::Deep
            | Self::Wide
            | Self::Truncated => vec![Source::Walker],
            Self::NestedEarly | Self::NestedLate | Self::ParserFiles | Self::ParserFolders => {
                vec![Source::FileList]
            }
            _ => vec![Source::FileList, Source::Walker],
        }
    }
    pub(super) fn shape(self) -> Shape {
        match self {
            Self::NestedEarly => Shape::NestedEarly,
            Self::NestedLate => Shape::NestedLate,
            Self::Deep => Shape::Deep,
            Self::Wide => Shape::Wide,
            Self::Links => Shape::InternalLinks,
            Self::StableSelective
            | Self::StableDense
            | Self::StableEdit
            | Self::Warm
            | Self::Promotion
            | Self::TabChain
            | Self::Reclaim
            | Self::WarmReclaim
            | Self::Preview
            | Self::Truncated => Shape::FlatFiles,
            _ => Shape::FlatMixed,
        }
    }
    pub(super) fn filter(self, condition: bool) -> Filter {
        Filter {
            files: !matches!(self, Self::Folders | Self::ParserFolders),
            dirs: !matches!(self, Self::Files | Self::EditFiles | Self::ParserFiles),
            ignore_enabled: match self {
                Self::Ignore | Self::MidIgnore => condition,
                Self::IgnoreCase | Self::SearchIgnore => true,
                _ => false,
            },
            ignore_case: self != Self::IgnoreCase || condition,
        }
    }
    pub(super) fn query(self, condition: bool) -> &'static str {
        if !condition {
            return "";
        }
        match self {
            Self::SearchIgnore | Self::StableSelective => "needle",
            Self::StableDense => "item",
            _ => "",
        }
    }
    pub(super) fn sort(self, condition: bool) -> (ResultSortMode, ResultSortScope) {
        if !condition {
            return (ResultSortMode::Score, ResultSortScope::ShownResults);
        }
        (
            match self {
                Self::NameShown | Self::NameAll => ResultSortMode::NameAsc,
                Self::ModifiedShown | Self::ModifiedAll => ResultSortMode::ModifiedDesc,
                _ => ResultSortMode::Score,
            },
            if matches!(self, Self::NameAll | Self::ModifiedAll) {
                ResultSortScope::AllMatches
            } else {
                ResultSortScope::ShownResults
            },
        )
    }
    pub(super) fn stable(self) -> bool {
        matches!(
            self,
            Self::StableSelective | Self::StableDense | Self::StableEdit | Self::Preview
        )
    }
    pub(super) fn tabs(self) -> bool {
        self.stable()
            || matches!(
                self,
                Self::Warm | Self::Promotion | Self::TabChain | Self::WarmReclaim
            )
    }
    pub(super) fn parser(self) -> bool {
        matches!(self, Self::ParserFiles | Self::ParserFolders)
    }
    pub(super) fn comparison_kind(self) -> &'static str {
        if matches!(
            self,
            Self::Files
                | Self::Folders
                | Self::Reclaim
                | Self::NestedEarly
                | Self::NestedLate
                | Self::Links
                | Self::Deep
                | Self::Wide
                | Self::Truncated
                | Self::ParserFiles
                | Self::ParserFolders
        ) {
            "AA-variability"
        } else {
            "AB-operation-cost"
        }
    }
    pub(super) fn condition_description(self, condition: bool) -> String {
        if self.comparison_kind() == "AA-variability" {
            format!("identical {} fixture/settings/refresh", self.name())
        } else if condition {
            format!(
                "{} actual operation; intended extra requests are recorded",
                self.name()
            )
        } else {
            format!(
                "matched {} fixture/prestate/source with operation disabled",
                self.name()
            )
        }
    }
}
