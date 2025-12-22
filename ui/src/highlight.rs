use std::collections::HashMap;

/// Highlight attributes from hl_attr_define
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
}

impl HighlightMap {
    pub fn new() -> Self {
        Self {
            attrs: HashMap::new(),
        }
    }

    /// Store a highlight definition from hl_attr_define
    pub fn define(&mut self, id: u32, attr: HighlightAttr) {
        self.attrs.insert(id, attr);
    }

    /// Get highlight attributes by ID
    #[allow(dead_code)]
    pub fn get(&self, id: u32) -> Option<&HighlightAttr> {
        self.attrs.get(&id)
    }
}

impl Default for HighlightMap {
    fn default() -> Self {
        Self::new()
    }
}
