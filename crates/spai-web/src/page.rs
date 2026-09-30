//! The browser page: where it is, what it keeps, and leaving it.

use serde::{de::DeserializeOwned, Serialize};

fn window() -> web_sys::Window {
    web_sys::window().expect("a browser window")
}

/// `https://eve-spai.com`
pub fn origin() -> String {
    window().location().origin().unwrap_or_default()
}

pub fn path() -> String {
    window().location().pathname().unwrap_or_default()
}

pub fn query() -> String {
    window().location().search().unwrap_or_default()
}

/// After `#`, without it.
pub fn fragment() -> String {
    window().location().hash().unwrap_or_default().trim_start_matches('#').to_owned()
}

/// Where the app is served from, ending in `/`: `/wh/` on the site, `/` in development.
pub fn base() -> String {
    let p = path();
    match p.find("/wh/") {
        Some(i) => p[..i + 4].to_owned(),
        None => "/".to_owned(),
    }
}

/// Shows `path` in the address bar without loading it, dropping the query and anything after `#`.
pub fn replace_path(path: &str) {
    if let Ok(h) = window().history() {
        let _ = h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
    }
}

pub fn go(url: &str) {
    let _ = window().location().set_href(url);
}

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    serde_json::from_str(&storage()?.get_item(key).ok()??).ok()
}

pub fn save<T: Serialize>(key: &str, value: &T) {
    if let (Some(s), Ok(v)) = (storage(), serde_json::to_string(value)) {
        let _ = s.set_item(key, &v);
    }
}

pub fn forget(key: &str) {
    if let Some(s) = storage() {
        let _ = s.remove_item(key);
    }
}
