use brz_json::{FieldState, JsonReader, OpaqueJson};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::io::Write;

fn segmented(raw: &str, size: usize) -> JsonReader<'static> {
    let arena = brz_io::EphemeralBytesArena::new(size);
    let mut writer = brz_io::Writer::new(&arena);
    writer.write_all(raw.as_bytes()).unwrap();
    JsonReader::new(writer.into_reader())
}

#[brz_json::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Update {
    #[json(presence)]
    title: String,
    #[json(presence)]
    progress: i64,
    #[json(presence, lenient)]
    legacy: String,
    ordinary: Option<bool>,
}

#[test]
fn model_fields_preserve_presence_and_serialize_missing_null_values() {
    for size in [1, 3, 4096] {
        for raw in ["{}", r#"{"title":null}"#, r#"{"title":"abc","progress":0}"#] {
            let reader = segmented(raw, size);
            let model: Update = reader.decode().unwrap();
            let original: serde_json::Value = brz_json::from_str(raw).unwrap();
            assert_eq!(model.title.is_missing(), original.get("title").is_none());
            assert_eq!(
                model.title.is_null(),
                original.get("title") == Some(&serde_json::Value::Null)
            );
            let mut expected = original;
            expected
                .as_object_mut()
                .unwrap()
                .insert("ordinary".into(), serde_json::Value::Null);
            assert_eq!(brz_json::to_value(&model).unwrap(), expected);
        }
    }
    let model: Update = brz_json::from_str(r#"{"title":"abc"}"#).unwrap();
    assert_eq!(model.title.value().map(String::as_str), Some("abc"));
    assert_eq!(model.title.into_value().as_deref(), Some("abc"));
}

#[test]
fn strict_and_lenient_fields_have_distinct_error_policies() {
    for raw in [r#"{"title":12}"#, r#"{"title":[]}"#, r#"{"progress":true}"#] {
        assert!(brz_json::from_str::<Update>(raw).is_err());
        assert!(segmented(raw, 1).decode::<Update>().is_err());
    }
    for raw in [r#"{"legacy":12}"#, r#"{"legacy":{"nested":[1,true]}}"#] {
        let reader = segmented(raw, 1);
        let model: Update = reader.decode().unwrap();
        assert!(model.legacy.is_present());
        assert!(!model.legacy.is_null());
        assert!(model.legacy.value().is_none());
    }
    assert!(
        segmented(r#"{"legacy":[1,]}"#, 1)
            .decode::<Update>()
            .is_err()
    );
}

#[brz_json::model]
#[derive(Debug, Serialize, Deserialize)]
struct Borrowed<'a> {
    #[json(presence)]
    #[serde(borrow, rename = "displayName")]
    name: &'a str,
}

#[brz_json::model]
#[derive(Debug, Serialize, Deserialize)]
struct Generic<T> {
    #[json(presence)]
    #[serde(default, rename = "data")]
    item: T,
}

#[test]
fn generic_models_and_null_accepting_types_keep_field_state() {
    let missing: Generic<OpaqueJson> = brz_json::from_str("{}").unwrap();
    assert!(missing.item.is_missing());
    let null: Generic<OpaqueJson> = segmented(r#"{"data":null}"#, 1).decode().unwrap();
    assert!(null.item.is_null());
    assert!(null.item.value().is_none());
    assert_eq!(brz_json::to_string(&null).unwrap(), r#"{"data":null}"#);
    let value: Generic<u8> = brz_json::from_slice(br#"{"data":7}"#).unwrap();
    assert_eq!(value.item.value(), Some(&7));
}

#[test]
fn presence_fields_support_renames_borrowing_and_mutation() {
    let reader = segmented(r#"{"displayName":"a\u0062"}"#, 1);
    let model: Borrowed<'_> = reader.decode().unwrap();
    assert_eq!(model.name.value(), Some(&"ab"));
    let mut model: Update = brz_json::from_str("{}").unwrap();
    model.title.set("x".into());
    assert_eq!(model.title.value().map(String::as_str), Some("x"));
    model.title.set_null();
    assert!(model.title.is_null());
    model.title.unset();
    assert!(model.title.is_missing());
}

#[test]
fn raw_json_preserves_exact_text_and_borrows_contiguous_input() {
    #[derive(Deserialize, Serialize)]
    struct Envelope<'a> {
        #[serde(borrow)]
        payload: &'a RawValue,
        tail: u8,
    }
    let raw = r#"{"payload":{ "z": 1.00, "a":"\u0062", "x":1e400 },"tail":7}"#;
    for size in [1, 2, 7, 4096] {
        let reader = segmented(raw, size);
        let model: Envelope<'_> = reader.decode().unwrap();
        assert_eq!(
            model.payload.get(),
            r#"{ "z": 1.00, "a":"\u0062", "x":1e400 }"#
        );
        assert_eq!(model.tail, 7);
        assert_eq!(
            brz_json::to_string(&model).unwrap(),
            format!(r#"{{"payload":{},"tail":7}}"#, model.payload.get())
        );
        let reader = segmented(raw, size);
        let owned: Box<RawValue> = reader.decode().unwrap();
        assert_eq!(owned.get(), raw);
    }
    let arena = brz_io::EphemeralBytesArena::new(4096);
    let mut writer = brz_io::Writer::new(&arena);
    writer.write_all(b"PREFIX[1, 2]NEXT").unwrap();
    let reader = writer.into_reader();
    let json = JsonReader::from_view(reader.view().slice(6..12).unwrap());
    let value: &RawValue = json.decode().unwrap();
    assert_eq!(value.get(), "[1, 2]");
    assert_eq!(
        value.get().as_ptr(),
        reader.view().peek_bytes(6, 6).unwrap().as_ptr()
    );
    assert_eq!(reader.position(), 0);
}

#[test]
fn raw_capture_rejects_bad_json_trailing_data_and_excess_depth() {
    for raw in [
        "",
        "[1,]",
        r#"{"a":1,}"#,
        r#""\uD800""#,
        "[00]",
        "[1 2]",
        "true false",
        "{1:2}",
        r#"{"a":tru}"#,
    ] {
        assert!(
            segmented(raw, 1).decode::<Box<RawValue>>().is_err(),
            "{raw}"
        );
    }
    let reader = segmented("[[[0]]]", 1);
    // The same recursion limit applies to raw capture and typed decoding.
    let source = reader.decode::<Box<RawValue>>().unwrap();
    let arena = brz_io::EphemeralBytesArena::new(1);
    let mut writer = brz_io::Writer::new(&arena);
    writer.write_all(source.get().as_bytes()).unwrap();
    assert!(
        JsonReader::with_recursion_limit(writer.into_reader(), 2)
            .decode::<Box<RawValue>>()
            .is_err()
    );
}

#[test]
fn opaque_json_retains_normalized_legacy_number_validation() {
    for raw in [
        "null",
        "1.0",
        "-0.0",
        "18446744073709551615",
        r#"{ "x":null,"nested":{"v":[]} }"#,
        "1e400",
        "invalid",
    ] {
        let old = serde_json::from_str::<serde_json::Value>(raw);
        let actual = brz_json::from_str::<OpaqueJson>(raw);
        assert_eq!(old.is_ok(), actual.is_ok(), "{raw}");
        if let (Ok(old), Ok(actual)) = (old, actual) {
            assert_eq!(
                serde_json::to_string(&old).unwrap(),
                brz_json::to_string(&actual).unwrap()
            );
        }
    }
    let value = OpaqueJson::from_serializable(serde_json::json!({"x": "abc"}));
    #[derive(Deserialize)]
    struct View<'a> {
        x: &'a str,
    }
    assert_eq!(value.decode::<View<'_>>().unwrap().x, "abc");
    assert!(value.is_nonempty_object());
    assert!(
        OpaqueJson::from_serializable(1)
            .decode::<View<'_>>()
            .is_err()
    );
    assert_eq!(brz_json::to_string(&brz_json::JsonNull).unwrap(), "null");
}

#[derive(Debug, Deserialize)]
struct Nested {
    items: Vec<Count>,
}
#[derive(Debug, Deserialize)]
struct Count {
    count: u8,
}
#[test]
fn error_paths_are_available_for_contiguous_and_segmented_inputs() {
    let raw = r#"{"items":[{"count":1},{"count":"bad"}]}"#;
    let error = brz_json::from_str::<Nested>(raw).unwrap_err();
    assert_eq!(error.path(), Some("items[1].count"));
    assert_eq!(
        error.to_string(),
        serde_json::from_str::<Nested>(raw).unwrap_err().to_string()
    );
    let error = segmented(raw, 1).decode::<Nested>().unwrap_err();
    assert_eq!(error.path(), Some("items[1].count"));
    assert!(error.byte_offset().is_some());
    let model: Nested = brz_json::from_str(r#"{"items":[{"count":1}]}"#).unwrap();
    assert_eq!(model.items[0].count, 1);
    assert!(brz_json::from_slice::<u8>(b"1 2").is_err());
}

#[test]
fn ordered_objects_keep_order_replace_duplicates_and_require_maps() {
    let object: brz_json::Object<u8> = brz_json::from_str(r#"{"z":1,"a":2,"z":3}"#).unwrap();
    assert_eq!(brz_json::to_string(&object).unwrap(), r#"{"z":3,"a":2}"#);
    assert_eq!(object.get("z"), Some(&3));
    assert!(brz_json::from_str::<brz_json::Object<u8>>("[]").is_err());
    let reader = segmented(r#"{"z":1,"a":2}"#, 1);
    assert_eq!(
        brz_json::to_string(&reader.decode::<brz_json::Object<u8>>().unwrap()).unwrap(),
        r#"{"z":1,"a":2}"#
    );
}
