use super::*;
use serde_json::json;

#[test]
fn rule_and_path_modes_accept_only_their_string_representation() {
    let enumerated = json!({"kind":"enum","fields":["/value"],"arguments":{"values":["current"]}});
    validate(&json!([enumerated]), &json!({"value":"current"})).unwrap();
    let mut object_kind = enumerated;
    object_kind["kind"] = json!({"enum":null});
    assert_eq!(
        validate(&json!([object_kind]), &json!({"value":"current"}))
            .unwrap_err()
            .reason,
        Reason::InvalidMapping
    );

    let path = json!({"base":"root","value":"state","fallback":null,"suffix":"","encoding":"path"});
    let rule = |path| json!([{"kind":"root-state-paths","fields":[],"arguments":{"paths":[path],"protected":[]}}]);
    validate(&rule(path.clone()), &json!({})).unwrap();
    for (field, object) in [
        ("base", json!({"root":null})),
        ("encoding", json!({"path":null})),
    ] {
        let mut invalid = path.clone();
        invalid[field] = object;
        assert_eq!(
            validate(&rule(invalid), &json!({})).unwrap_err().reason,
            Reason::InvalidMapping
        );
    }
}

#[test]
fn rule_decoding_and_address_failures_preserve_their_sources_privately() {
    use std::error::Error as _;
    let unknown = validate(
        &json!([{"kind":"private-unknown-rule","fields":[],"arguments":{}}]),
        &json!({}),
    )
    .unwrap_err();
    assert_eq!(unknown.reason, Reason::InvalidMapping);
    assert!(
        unknown
            .source()
            .unwrap()
            .source()
            .unwrap()
            .is::<serde_json::Error>()
    );
    let address = validate(
        &json!([{"kind":"socket-address","fields":["/bind"],"arguments":{}}]),
        &json!({"bind":"private-invalid-address"}),
    )
    .unwrap_err();
    assert_eq!(address.reason, Reason::InvalidValue);
    assert!(
        address
            .source()
            .unwrap()
            .source()
            .unwrap()
            .is::<std::net::AddrParseError>()
    );
    for failure in [unknown, address] {
        assert!(!format!("{failure:?}").contains("private-"));
        assert!(
            !serde_json::to_string(&failure.envelope())
                .unwrap()
                .contains("private-")
        );
    }
}

fn validate(rules: &Value, value: &Value) -> std::result::Result<(), ConfigError> {
    validate_rules(rules, value, Path::new("/opt/fixture"), ConfigSource::File)
}

#[test]
fn optional_paths_reject_malformed_rule_arguments() {
    assert_eq!(
        validate(
            &json!([{"kind":"path","fields":["/path"],"arguments":{"optional":true,"absolute":"invalid"}}]),
            &json!({"path":null}),
        )
        .unwrap_err()
        .reason,
        Reason::InvalidMapping,
    );
    validate(
        &json!([{"kind":"path","fields":["/path"],"arguments":{"optional":true,"absolute":true}}]),
        &json!({"path":null}),
    )
    .unwrap();
}

#[test]
fn current_socket_and_enum_rules_accept_boundaries_and_reject_unsafe_values() {
    let rules = json!([
        {"kind":"enum","fields":["/mode"],"arguments":{"values":["off","on"]}},
        {"kind":"socket-address","fields":["/bind"],"arguments":{"loopback_when":"/development","nonzero":true}}
    ]);
    let valid = json!({"mode":"on","bind":"[::1]:8080","development":true});
    validate(&rules, &valid).unwrap();
    for (field, bad) in [
        ("mode", json!("unknown")),
        ("bind", json!("0.0.0.0:8080")),
        ("bind", json!("[::1]:0")),
    ] {
        let mut value = valid.clone();
        value[field] = bad;
        let error = validate(&rules, &value).unwrap_err();
        assert_eq!(error.source, ConfigSource::File);
        assert!(!serde_json::to_string(&error).unwrap().contains("unknown"));
    }
}

#[test]
fn state_paths_resolve_fallbacks_and_reject_reserved_or_overlapping_namespaces() {
    let paths = json!([
        {"base":"config","value":"/data","fallback":{"base":"root","value":"state","fallback":null,"suffix":"","encoding":"path"},"suffix":"","encoding":"path"},
        {"base":"config","value":"/media","fallback":null,"suffix":"","encoding":"path"}
    ]);
    let rules = json!([{"kind":"root-state-paths","fields":[],"arguments":{"paths":paths,"protected":crate::server_layout::ROOT_RESERVED_DIRECTORIES}}]);
    validate(&rules, &json!({"data":null,"media":"/srv/video"})).unwrap();
    for media in [
        "/opt/fixture/bin/videos",
        "/opt/fixture",
        "/opt/fixture/state/sub",
        "/opt/fixture/../escape",
        "/",
    ] {
        assert!(validate(&rules, &json!({"data":null,"media":media})).is_err());
    }
    let sqlite = json!([{"kind":"root-state-paths","fields":[],"arguments":{"paths":[{"base":"config","value":"/database","fallback":null,"suffix":"","encoding":"sqlite-url"}],"protected":crate::server_layout::ROOT_RESERVED_DIRECTORIES}}]);
    validate(
        &sqlite,
        &json!({"database":"sqlite:///srv/data/current.db"}),
    )
    .unwrap();
    assert!(
        validate(
            &sqlite,
            &json!({"database":"sqlite:///srv/data/current.db?mode=rw"})
        )
        .is_err()
    );
}

#[test]
fn external_assets_require_explicit_development_and_unbound_source() {
    for bound in [false, true] {
        let rule = json!([{"kind":"external-web-override","fields":["/web"],"arguments":{"development_pointer":"/development","source_bound":bound}}]);
        for development in [false, true] {
            validate(&rule, &json!({"web":null,"development":development})).unwrap();
            assert_eq!(
                validate(
                    &rule,
                    &json!({"web":"/tmp/assets","development":development})
                )
                .is_ok(),
                development && !bound
            );
        }
    }
}

#[test]
fn unknown_rules_arguments_missing_values_and_definition_bounds_fail_closed() {
    for rule in [
        json!({"kind":"future-rule","fields":[],"arguments":{}}),
        json!({"kind":"path","fields":["/path"],"arguments":{"allow_unsafe":true}}),
        json!({"kind":"enum","fields":["/path"],"arguments":{"values":[{}]}}),
        json!({"kind":"path","fields":["/x~9"],"arguments":{}}),
    ] {
        assert_eq!(
            validate(&json!([rule]), &json!({"path":"/tmp"}))
                .unwrap_err()
                .reason,
            Reason::InvalidMapping
        );
    }
    let missing = validate(
        &json!([{"kind":"path","fields":["/missing"],"arguments":{}}]),
        &json!({}),
    )
    .unwrap_err();
    assert_eq!(missing.reason, Reason::MissingField);
    let oversized = Value::Array(vec![
        json!({"kind":"path","fields":["/path"],"arguments":{}});
        MAX_RULES + 1
    ]);
    assert!(validate(&oversized, &json!({"path":"/tmp"})).is_err());
}

#[test]
fn declared_path_relationship_preserves_allowed_subtrees_and_protects_private_entries() {
    let spec = |pointer| json!({"base":"config","value":pointer,"fallback":null,"suffix":"","encoding":"path"});
    for allow in [false, true] {
        let rules = json!([{"kind":"path-relation","fields":[],"arguments":{"parent":spec("/private"),"child":spec("/business"),"allow_child_subtree":allow,"forbidden_child_first_component_prefixes":["current.db","secrets.json",".state-"]}}]);
        validate(
            &rules,
            &json!({"private":"/srv/private","business":"/srv/business"}),
        )
        .unwrap();
        assert_eq!(
            validate(
                &rules,
                &json!({"private":"/srv/private","business":"/srv/private/media"})
            )
            .is_ok(),
            allow
        );
        for business in [
            "/srv/private",
            "/srv",
            "/srv/private/current.db-wal",
            "/srv/private/secrets.json/sub",
            "/srv/private/.state-instance.lock",
        ] {
            assert!(
                validate(
                    &rules,
                    &json!({"private":"/srv/private","business":business})
                )
                .is_err()
            );
        }
    }
}
