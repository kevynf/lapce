use floem::views::editor::text::RenderWhitespace;
use serde::{Deserialize, Serialize};
use structdesc::FieldNames;

pub const SCALE_OR_SIZE_LIMIT: f64 = 5.0;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ClickMode {
    #[default]
    #[serde(rename = "single")]
    SingleClick,
    #[serde(rename = "file")]
    DoubleClickFile,
    #[serde(rename = "all")]
    DoubleClickAll,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum WrapStyle {
    /// No wrapping
    None,
    /// Wrap at the editor width
    #[default]
    EditorWidth,
    /// Wrap at a specific width
    WrapWidth,
}
impl WrapStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            WrapStyle::None => "none",
            WrapStyle::EditorWidth => "editor-width",
            WrapStyle::WrapWidth => "wrap-width",
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "none" => Some(WrapStyle::None),
            "editor-width" => Some(WrapStyle::EditorWidth),
            "wrap-width" => Some(WrapStyle::WrapWidth),
            _ => None,
        }
    }
}

impl std::fmt::Display for WrapStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())?;

        Ok(())
    }
}

#[derive(FieldNames, Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct EditorConfig {
    #[field_names(desc = "Set the editor font family")]
    pub font_family: String,
    #[field_names(desc = "Set the editor font size")]
    font_size: usize,
    #[field_names(
        desc = "Set the editor line height. If less than 5.0, line height will be a multiple of the font size."
    )]
    line_height: f64,
    #[field_names(
        desc = "If enabled, when you input a tab character, it will insert indent that's detected based on your files."
    )]
    pub smart_tab: bool,
    #[field_names(desc = "Set the tab width")]
    pub tab_width: usize,
    #[field_names(desc = "If opened editors are shown in a tab")]
    pub show_tab: bool,
    #[field_names(desc = "If navigation breadcrumbs are shown for the file")]
    pub show_bread_crumbs: bool,
    #[field_names(desc = "If the editor can scroll beyond the last line")]
    pub scroll_beyond_last_line: bool,
    #[field_names(
        desc = "Set the minimum number of visible lines above and below the cursor"
    )]
    pub cursor_surrounding_lines: usize,
    #[field_names(desc = "The kind of wrapping to perform")]
    pub wrap_style: WrapStyle,
    #[field_names(desc = "The number of pixels to wrap at")]
    pub wrap_width: usize,
    #[field_names(
        desc = "Show code context like functions and classes at the top of editor when scroll"
    )]
    pub sticky_header: bool,
    #[field_names(
        desc = "Whether the editor should enable automatic closing of matching pairs"
    )]
    pub auto_closing_matching_pairs: bool,
    #[field_names(
        desc = "Whether the editor should automatically surround selected text when typing quotes or brackets"
    )]
    pub auto_surround: bool,
    #[field_names(
        desc = "If modal mode should have relative line numbers (though, not in insert mode)"
    )]
    pub modal_mode_relative_line_numbers: bool,
    #[field_names(
        desc = "Whether newlines should be automatically converted to the current line ending"
    )]
    pub normalize_line_endings: bool,

    #[field_names(desc = "If matching brackets are highlighted")]
    pub highlight_matching_brackets: bool,

    #[field_names(desc = "If scope lines are highlighted")]
    pub highlight_scope_lines: bool,
    #[field_names(
        desc = "Set the cursor blink interval (in milliseconds). Set to 0 to completely disable."
    )]
    blink_interval: u64,
    #[field_names(
        desc = "Whether the multiple cursor selection is case sensitive."
    )]
    pub multicursor_case_sensitive: bool,
    #[field_names(
        desc = "Whether the multiple cursor selection only selects whole words."
    )]
    pub multicursor_whole_words: bool,
    #[field_names(
        desc = "How the editor should render whitespace characters.\nOptions: none, all, boundary, trailing."
    )]
    pub render_whitespace: RenderWhitespace,
    #[field_names(desc = "Whether the editor show indent guide.")]
    pub show_indent_guide: bool,
    #[field_names(
        desc = "Set the auto save delay (in milliseconds), Set to 0 to completely disable"
    )]
    pub autosave_interval: u64,
    #[field_names(
        desc = "If enabled the cursor treats leading soft tabs as if they are hard tabs."
    )]
    pub atomic_soft_tabs: bool,
    #[field_names(
        desc = "Use a double click to interact with the file explorer.\nOptions: single (default), file or all."
    )]
    pub double_click: ClickMode,
    #[field_names(desc = "Whether the editor colorizes brackets")]
    pub bracket_pair_colorization: bool,
    #[field_names(desc = "Bracket colorization Limit")]
    pub bracket_colorization_limit: u64,
    #[field_names(
        desc = "Glob patterns for excluding files and folders (in file explorer)"
    )]
    pub files_exclude: String,
}

impl EditorConfig {
    pub fn font_size(&self) -> usize {
        self.font_size.clamp(6, 32)
    }

    pub fn line_height(&self) -> usize {
        let line_height = if self.line_height < SCALE_OR_SIZE_LIMIT {
            self.line_height * self.font_size as f64
        } else {
            self.line_height
        };

        // Prevent overlapping lines
        (line_height.round() as usize).max(self.font_size)
    }

    pub fn blink_interval(&self) -> u64 {
        if self.blink_interval == 0 {
            return 0;
        }
        self.blink_interval.max(200)
    }
}
