//! What the tab is showing. Grows into the full state as the backend lands; for now it is the page
//! the sub-nav is on.

/// Which page of the tab is open. A detail page carries the fleet it opened, so going back is just
/// setting this to `Fleets`.
// The detail pages are reachable only from the fleet list, which is not built yet.
#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum Page {
    #[default]
    Fleets,
    Start,
    Tracking(String),
    Historic(String),
}

impl Page {
    /// The sub-nav entry that should read as selected, since a detail page belongs to a tab.
    pub fn tab(&self) -> Page {
        match self {
            Page::Tracking(_) | Page::Historic(_) => Page::Fleets,
            other => other.clone(),
        }
    }
}

#[derive(Default)]
pub struct FleetState {
    pub page: Page,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fleet's own page still belongs to the Fleets tab, or the sub-nav loses its highlight the
    /// moment you open one.
    #[test]
    fn a_detail_page_belongs_to_the_list_tab() {
        assert_eq!(Page::Tracking("abc".into()).tab(), Page::Fleets);
        assert_eq!(Page::Historic("abc".into()).tab(), Page::Fleets);
        assert_eq!(Page::Start.tab(), Page::Start);
    }
}
