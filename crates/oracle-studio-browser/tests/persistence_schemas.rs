use serde_json::Value;

fn schema(source: &str) -> Value {
    serde_json::from_str(source).expect("tracked schema is valid JSON")
}

#[test]
fn public_settings_schema_keeps_identity_scope_policy_and_payload_explicit() {
    let schema = schema(include_str!(
        "../../../schemas/local-settings-bundle-v1.schema.json"
    ));
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
    assert_eq!(schema["properties"]["objects"]["maxItems"], 4096);
    assert_eq!(
        schema["$defs"]["public_object"]["additionalProperties"],
        false
    );
    let required = schema["$defs"]["public_object"]["required"]
        .as_array()
        .expect("required fields");
    for field in [
        "scope",
        "object_type",
        "object_id",
        "schema_version",
        "revision",
        "parent_content_id",
        "content_id",
        "updated_at",
        "policy",
        "payload",
    ] {
        assert!(required.iter().any(|item| item == field), "missing {field}");
    }
    assert_eq!(
        schema["$defs"]["public_object"]["properties"]["policy"]["enum"],
        serde_json::json!(["device_local", "portable", "syncable"])
    );
}

#[test]
fn aggregate_backup_schema_embeds_exact_vault_bytes_without_redefining_them() {
    let schema = schema(include_str!(
        "../../../schemas/local-backup-container-v1.schema.json"
    ));
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
    assert_eq!(schema["properties"]["entries"]["maxItems"], 1024);
    let vault = &schema["$defs"]["encrypted_vault"];
    assert_eq!(vault["additionalProperties"], false);
    assert_eq!(
        vault["properties"]["media_type"]["const"],
        "application/vnd.oracle-studio.vault-v2"
    );
    assert_eq!(vault["properties"]["encoding"]["const"], "base64");
    assert_eq!(
        schema["$defs"]["public_settings"]["properties"]["document"]["$ref"],
        "local-settings-bundle-v1.schema.json"
    );
}
