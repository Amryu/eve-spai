//! System name fields that complete from the map.

use super::*;

impl SpaiApp {
    pub(crate) fn system_input(&mut self, ui: &mut egui::Ui, key: &'static str, q: &mut String, hint: &str, width: f32) -> Option<i64> {
        if self.sys_sugg.get(key).is_none_or(|(last, _, _)| last != q) {
            let hits = self.travel_suggestions(q);
            self.sys_sugg.insert(key, (q.clone(), hits, 0));
        }
        let (_, hits, sel) = self.sys_sugg.get_mut(key)?;
        let hits = hits.clone();
        let pick = super::map_panels::system_field(ui, q, sel, hint, width, &hits);
        if let Some(name) = pick.and_then(|id| self.systems.as_ref()?.info_of(id).map(|i| i.name.clone())) {
            *q = name;
        }
        pick
    }

    pub(crate) fn travel_suggestions(&self, q: &str) -> Vec<SysHit> {
        let q = q.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let Some(store) = self.store.as_ref() else { return Vec::new() };
        store
            .search_systems(q, 8)
            .into_iter()
            .map(|(id, name, sec)| {
                let (c, r) = self
                    .systems
                    .as_ref()
                    .and_then(|g| g.info_of(id))
                    .map(|i| (i.constellation.clone(), i.region.clone()))
                    .unwrap_or_default();
                (id, name, sec, c, r)
            })
            .collect()
    }
}
