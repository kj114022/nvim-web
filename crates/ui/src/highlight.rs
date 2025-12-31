use std::collections::HashMap;

/// Highlight attributes from `hl_attr_define`
#[derive(Clone, Default)]
pub struct HighlightAttr {
    pub fg: Option<u32>,  // RGB color
    pub bg: Option<u32>,  // RGB color
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Storage for highlight definitions
pub struct HighlightMap {
    attrs: HashMap<u32, HighlightAttr>,
    /// Default foreground color from `default_colors_set`
    pub default_fg: Option<u32>,
    /// Default background color from `default_colors_set`
    pub default_bg: Option<u32>,
}

impl HighlightMap {
    pub fn new() -> Self {
        Self {
            attrs: HashMap::new(),
            default_fg: None,
            default_bg: None,
        }
    }

    /// Store a highlight definition from `hl_attr_define`
    pub fn define(&mut self, id: u32, attr: HighlightAttr) {
        self.attrs.insert(id, attr);
    }

    /// Get highlight attributes by ID
    #[allow(dead_code)]
    pub fn get(&self, id: u32) -> Option<&HighlightAttr> {
        self.attrs.get(&id)
    }

    /// Set default colors from `default_colors_set` event
    pub fn set_default_colors(&mut self, fg: Option<u32>, bg: Option<u32>) {
        self.default_fg = fg;
        self.default_bg = bg;
    }
}

impl Default for HighlightMap {
    fn default() -> Self {
        Self::new()
    }
}
