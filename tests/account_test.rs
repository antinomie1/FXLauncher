use fxlauncher::account::{Account, AccountStore, offline_uuid};
use fxlauncher::net::auth::{AuthError, DeviceCode, xsts_error};
use fxlauncher::net::profile::{McProfile, validate_skin_png};

#[test]
fn test_offline_uuid_known_values() {
    // OfflinePlayer:Steve -> 5627dd98-e6be-3c21-b8a8-e92344183641
    // OfflinePlayer:Alex  -> 36532b5e-c442-3dbb-a24c-c7e55d0f979a
    let steve_uuid = offline_uuid("Steve").to_string();
    assert_eq!(steve_uuid, "5627dd98-e6be-3c21-b8a8-e92344183641");

    let alex_uuid = offline_uuid("Alex").to_string();
    assert_eq!(alex_uuid, "36532b5e-c442-3dbb-a24c-c7e55d0f979a");
}

#[test]
fn test_account_creation_and_properties() {
    let acc = Account::new_offline("Steve");
    assert_eq!(acc.username(), "Steve");
    assert_eq!(acc.uuid(), "5627dd98-e6be-3c21-b8a8-e92344183641");
    assert_eq!(acc.access_token(), "0");
    assert_eq!(acc.user_type(), "legacy");
    assert!(!acc.needs_refresh(u64::MAX));

    let ms_acc = microsoft("Alex", "12345678-1234-1234-1234-123456789abc", 10_000);
    assert_eq!(ms_acc.username(), "Alex");
    assert_eq!(ms_acc.access_token(), "secret_token");
    assert_eq!(ms_acc.user_type(), "msa");
    // Refreshed five minutes ahead of expiry.
    assert!(!ms_acc.needs_refresh(9_000));
    assert!(ms_acc.needs_refresh(9_700));
    assert!(ms_acc.needs_refresh(20_000));
}

fn microsoft(name: &str, uuid: &str, expires_at: u64) -> Account {
    Account::Microsoft {
        username: name.to_string(),
        uuid: uuid.to_string(),
        access_token: "secret_token".to_string(),
        refresh_token: "refresh".to_string(),
        expires_at,
    }
}

#[test]
fn test_account_store_upsert_remove_and_persist() {
    let mut store = AccountStore::default();
    assert!(store.selected().is_none());
    store.upsert(Account::new_offline("Steve"));
    store.upsert(microsoft("Alex", "aaaaaaaa-0000-4000-8000-000000000000", 1));
    assert_eq!(store.selected().unwrap().username(), "Alex");

    // Signing in again replaces the saved account instead of adding a second one.
    store.upsert(microsoft("Alex2", "aaaaaaaa-0000-4000-8000-000000000000", 2));
    assert_eq!(store.accounts.len(), 2);
    assert_eq!(store.selected().unwrap().username(), "Alex2");

    // Removing an account before the selection keeps the same account selected.
    store.upsert(Account::new_offline("Herobrine"));
    store.selected = 1;
    store.remove(offline_uuid("Steve").to_string().as_str());
    assert_eq!(store.selected().unwrap().username(), "Alex2");
    // Removing the selected last account moves the selection back.
    store.selected = 1;
    store.remove(offline_uuid("Herobrine").to_string().as_str());
    assert_eq!(store.selected().unwrap().username(), "Alex2");
    store.remove("aaaaaaaa-0000-4000-8000-000000000000");
    assert!(store.selected().is_none());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    store.upsert(microsoft("Alex", "aaaaaaaa-0000-4000-8000-000000000000", 7));
    store.save(&path).unwrap();
    assert_eq!(AccountStore::load(&path), store);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(AccountStore::load(&dir.path().join("missing.json")), AccountStore::default());
}

#[test]
fn test_profile_parsing() {
    let profile: McProfile = serde_json::from_str(r#"{
        "id": "069a79f444e94726a5befca90e38aaf5", "name": "Notch",
        "skins": [{"id": "s1", "state": "ACTIVE", "url": "http://textures.minecraft.net/texture/abc", "variant": "SLIM"}],
        "capes": [
            {"id": "c1", "state": "INACTIVE", "url": "http://textures.minecraft.net/texture/c1", "alias": "Migrator"},
            {"id": "c2", "state": "ACTIVE", "url": "http://textures.minecraft.net/texture/c2", "alias": "Vanilla"}
        ]
    }"#).unwrap();
    assert_eq!(profile.hyphenated_id(), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
    assert!(profile.active_skin().unwrap().is_slim());
    assert_eq!(profile.active_cape().unwrap().alias, "Vanilla");

    let bare: McProfile = serde_json::from_str(r#"{"id": "x", "name": "Dev"}"#).unwrap();
    assert!(bare.active_skin().is_none() && bare.capes.is_empty());
}

fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    png
}

#[test]
fn test_skin_validation() {
    assert!(validate_skin_png(&png_header(64, 64)).is_ok());
    assert!(validate_skin_png(&png_header(64, 32)).is_ok());
    assert!(validate_skin_png(&png_header(128, 128)).unwrap_err().contains("128×128"));
    assert!(validate_skin_png(b"GIF89a").is_err());
}

#[test]
fn test_auth_helpers() {
    assert!(matches!(xsts_error(2148916233), AuthError::NoXboxAccount));
    assert!(matches!(xsts_error(2148916238), AuthError::ChildAccount));
    assert!(matches!(xsts_error(42), AuthError::Xsts(42)));
    let code: DeviceCode = serde_json::from_str(r#"{"user_code":"ABCD1234","device_code":"x","verification_uri":"https://www.microsoft.com/link","expires_in":900,"interval":5,"message":"…"}"#).unwrap();
    assert_eq!(code.prefilled_uri(), "https://www.microsoft.com/link?otc=ABCD1234");
}

#[test]
fn test_username_validation() {
    assert!(Account::validate_username("Steve"));
    assert!(Account::validate_username("Player_123"));
    assert!(Account::validate_username("a"));
    assert!(Account::validate_username("1234567890123456")); // 16 chars

    // Invalid usernames
    assert!(!Account::validate_username("")); // empty
    assert!(!Account::validate_username("12345678901234567")); // 17 chars
    assert!(!Account::validate_username("Steve!")); // invalid char
    assert!(!Account::validate_username("Player Name")); // contains space
}
