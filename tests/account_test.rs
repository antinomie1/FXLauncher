use fxlauncher::account::{Account, offline_uuid};

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

    let from_cred = Account::from_credentials("Steve", true);
    assert_eq!(acc, from_cred);

    let ms_acc = Account::Microsoft {
        username: "Alex".to_string(),
        uuid: "12345678-1234-1234-1234-123456789abc".to_string(),
        access_token: "secret_token".to_string(),
    };
    assert_eq!(ms_acc.username(), "Alex");
    assert_eq!(ms_acc.access_token(), "secret_token");
    assert_eq!(ms_acc.user_type(), "msa");
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
