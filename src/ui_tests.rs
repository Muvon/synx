use super::*;

#[test]
fn selects_filters_and_renders_all_status_variants() {
    assert_eq!(default_filter(0, true), "synx=warn");
    assert_eq!(default_filter(0, false), "synx=info");
    assert_eq!(default_filter(1, false), "synx=debug");
    assert_eq!(default_filter(2, false), "synx=trace");

    let with_user = Remote {
        user: Some("dev".into()),
        host: "host".into(),
        path: "/remote".into(),
    };
    let without_user = Remote {
        user: None,
        host: "host".into(),
        path: "/remote".into(),
    };
    banner(Path::new("/local"), &with_user, SyncMode::Push);
    banner(Path::new("/local"), &without_user, SyncMode::Pull);
    banner(Path::new("/local"), &without_user, SyncMode::Both);
    ok("ready");
    info("working");
    warn("careful");
}
