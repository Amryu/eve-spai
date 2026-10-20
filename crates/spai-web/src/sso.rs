//! EVE SSO from the browser, shared with the fitting simulator (`spai_webkit::sso`).

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_imports))]
pub use spai_webkit::sso::{callback_code, Pending, Session, TokenReply};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_sign_in_is_the_one_this_app_registered() {
        let p = Pending::new();
        assert!(p.authorize_url("https://eve-spai.com/wh/callback").contains(&format!("client_id={}", spai_webkit::sso::CLIENT_ID)));
        assert_eq!(spai_webkit::sso::CLIENT_ID, "c0e78d5da88849ac94775034063ec68f");
    }
}
