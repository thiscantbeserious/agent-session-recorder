//! `StoreError` texts name the next step and stay backend neutral.

use agr::sync::testing::StoreError;

#[test]
fn store_error_messages_name_the_next_step() {
    assert_eq!(
        StoreError::Moved("idx".into()).to_string(),
        "changed on the remote, run again"
    );
    let msg = StoreError::RateLimited.to_string();
    assert!(msg.contains("run again later"), "{msg}");
    let msg = StoreError::NotFound.to_string();
    assert!(msg.contains("not found"), "{msg}");
}

#[test]
fn debug_of_moved_hides_the_index() {
    let shown = format!("{:?}", StoreError::Moved("secret".into()));
    assert!(!shown.contains("secret"), "{shown}");
}

#[test]
fn store_error_display_is_backend_neutral() {
    let all = [
        StoreError::NotInstalled("tool"),
        StoreError::NotLoggedIn,
        StoreError::MissingScope,
        StoreError::Unreachable("why".into()),
        StoreError::RateLimited,
        StoreError::NotFound,
    ];
    for e in all {
        let text = e.to_string();
        assert!(!text.contains("gh ") && !text.contains("GitHub"), "{text}");
    }
}
