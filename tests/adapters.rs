use brz_json::{JsonReader, adapters, from_str};
use serde::Deserialize;
use std::io::Write;

#[derive(Debug, Deserialize)]
struct Integer(#[serde(deserialize_with = "adapters::integer")] i64);
#[derive(Debug, Deserialize)]
struct Truncated(#[serde(deserialize_with = "adapters::truncated_integer")] i64);
#[derive(Debug, Deserialize)]
struct Number(#[serde(deserialize_with = "adapters::number")] f64);
#[derive(Debug, Deserialize)]
struct Bool(#[serde(deserialize_with = "adapters::boolean")] bool);
#[derive(Debug, Deserialize)]
struct LegacyBool(#[serde(deserialize_with = "adapters::legacy_boolean")] bool);

fn segmented<T: serde::de::DeserializeOwned>(raw: &str) -> brz_json::Result<T> {
    let arena = brz_io::EphemeralBytesArena::new(1);
    let mut writer = brz_io::Writer::new(&arena);
    writer.write_all(raw.as_bytes()).unwrap();
    JsonReader::new(writer.into_reader()).decode()
}

#[test]
fn integer_policies_cover_strings_whole_floats_bools_and_boundaries() {
    for (raw, expected) in [
        ("true", 1),
        ("false", 0),
        ("30.0", 30),
        (r#"" +42 ""#, 42),
        (r#""4.000""#, 4),
        ("-9223372036854775808", i64::MIN),
        ("9223372036854775807", i64::MAX),
    ] {
        assert_eq!(from_str::<Integer>(raw).unwrap().0, expected);
        assert_eq!(segmented::<Integer>(raw).unwrap().0, expected);
    }
    for raw in [
        "30.5",
        r#""30.5""#,
        r#""4.""#,
        r#"".5""#,
        r#""1e3""#,
        "1e300",
        "9.223372036854776e18",
        "-9.223372036854776e18",
        "18446744073709551615",
        "null",
        "[]",
    ] {
        assert!(from_str::<Integer>(raw).is_err(), "{raw}");
        assert!(segmented::<Integer>(raw).is_err(), "{raw}");
    }
    assert_eq!(from_str::<Truncated>("-30.9").unwrap().0, -30);
    assert_eq!(from_str::<Number>(r#"" 2.5 ""#).unwrap().0, 2.5);
    assert_eq!(segmented::<Number>("true").unwrap().0, 1.0);
}

#[test]
fn boolean_coercion_and_python_truthiness_are_distinct() {
    for raw in ["true", "1", "1.0", r#""YES""#, r#""t""#, r#""on""#] {
        assert!(from_str::<Bool>(raw).unwrap().0);
        assert!(segmented::<Bool>(raw).unwrap().0);
    }
    for raw in ["false", "0", "0.0", r#""FALSE""#, r#""n""#] {
        assert!(!from_str::<Bool>(raw).unwrap().0);
    }
    for raw in ["2", r#""maybe""#, r#""""#, r#"" true ""#, "null", "[]"] {
        assert!(from_str::<Bool>(raw).is_err(), "{raw}");
    }
    assert!(from_str::<LegacyBool>("2").unwrap().0);
    assert!(!from_str::<LegacyBool>(r#""""#).unwrap().0);
    assert!(from_str::<LegacyBool>(r#"" true ""#).unwrap().0);
    assert!(from_str::<LegacyBool>("1.0").is_err());
    for raw in ["null", "false", "0", "0.0", r#""""#, "[]", "{}"] {
        assert!(!segmented::<adapters::PythonTruthiness>(raw).unwrap().0);
        assert!(!adapters::python_truthy(&from_str(raw).unwrap()));
    }
    for raw in [r#""false""#, "[null]", r#"{"x":null}"#, "-1"] {
        assert!(segmented::<adapters::PythonTruthiness>(raw).unwrap().0);
        assert!(adapters::python_truthy(&from_str(raw).unwrap()));
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct Defaults {
    #[serde(default, deserialize_with = "adapters::null_as_default")]
    names: Vec<String>,
    #[serde(default, deserialize_with = "adapters::invalid_as_default")]
    text: String,
    #[serde(default, deserialize_with = "adapters::optional_integer")]
    count: Option<i64>,
    #[serde(default, deserialize_with = "adapters::nonempty_string")]
    employee: Option<String>,
}

#[test]
fn defaults_missing_null_wrong_type_and_empty_values_remain_separate() {
    let absent: Defaults = from_str("{}").unwrap();
    let null: Defaults = segmented(r#"{"names":null,"text":12,"employee":""}"#).unwrap();
    assert_eq!(absent, null);
    assert!(from_str::<Defaults>(r#"{"names":12}"#).is_err());
    assert!(from_str::<Defaults>(r#"{"employee":12}"#).is_err());
    assert_eq!(
        from_str::<Defaults>(r#"{"count":false}"#).unwrap().count,
        Some(0)
    );
    assert_eq!(
        from_str::<Defaults>(r#"{"employee":"abc"}"#)
            .unwrap()
            .employee
            .as_deref(),
        Some("abc")
    );
    assert!(segmented::<Defaults>(r#"{"text":[1,]}"#).is_err());
}

#[derive(Debug, Deserialize)]
struct Model {
    name: String,
}
#[derive(Debug, Deserialize)]
struct Objects {
    #[serde(deserialize_with = "adapters::optional_object")]
    model: Option<Model>,
    #[serde(deserialize_with = "adapters::optional_objects")]
    models: Option<Vec<Model>>,
    #[serde(deserialize_with = "adapters::nonnull_items")]
    names: Vec<String>,
    #[serde(deserialize_with = "adapters::optional_integers")]
    integers: Option<Vec<i64>>,
}
#[test]
fn objects_require_mapping_shape_and_lists_have_explicit_element_policies() {
    let value: Objects = segmented(r#"{"model":{"name":"a"},"models":[{"name":"b"}],"names":[null,"", "c"],"integers":[1,"2",true,3.0]}"#).unwrap();
    assert_eq!(value.model.unwrap().name, "a");
    assert_eq!(value.models.unwrap()[0].name, "b");
    assert_eq!(value.names, ["", "c"]);
    assert_eq!(value.integers, Some(vec![1, 2, 1, 3]));
    for raw in [
        r#"{"model":["a"],"models":null,"names":[],"integers":null}"#,
        r#"{"model":null,"models":[["a"]],"names":[],"integers":null}"#,
        r#"{"model":null,"models":null,"names":[12],"integers":null}"#,
    ] {
        assert!(segmented::<Objects>(raw).is_err());
    }
}

#[test]
fn tolerant_projection_keeps_document_validation_and_preserves_later_fields() {
    #[derive(Deserialize)]
    struct Input {
        #[serde(deserialize_with = "adapters::try_decode")]
        model: Option<Model>,
        tail: u8,
    }
    let value: Input = segmented(r#"{"model":{"name":false,"other":[1,2]},"tail":7}"#).unwrap();
    assert!(value.model.is_none());
    assert_eq!(value.tail, 7);
    assert!(segmented::<Input>(r#"{"model":{"name":1e400},"tail":7}"#).is_err());
    assert!(segmented::<Input>(r#"{"model":{"name":[1,]},"tail":7}"#).is_err());
    assert!(segmented::<brz_json::JsonProjection<Model>>("1e400").is_err());
    assert!(
        segmented::<brz_json::JsonProjection<Model>>("null")
            .unwrap()
            .value
            .is_none()
    );
}
