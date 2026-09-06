use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use brz_ds::EphemeralBytesArena;
use brz_io::{Reader, Writer};
use brz_json::JsonReader;
use serde::{Deserialize, Serialize};

fn input(bytes: &[u8], segment: usize) -> Reader {
    let arena = EphemeralBytesArena::new(segment);
    let mut writer = Writer::with_limit(&arena, bytes.len());
    writer.write_all(bytes).unwrap();
    writer.into_reader()
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Child<'a> {
    name: &'a str,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Request<'a> {
    name: &'a str,
    escaped: &'a str,
    #[serde(borrow)]
    children: Vec<Child<'a>>,
    active: bool,
    count: i64,
    optional: Option<u32>,
}

#[test]
fn derived_struct_borrows_multiple_fields_including_escapes() {
    let bytes = br#"{"name":"alice","escaped":"a\u0062\n\"\\","children":[{"name":"\u4f60\u597d"},{"name":"\uD83D\uDE00"}],"active":true,"count":-42,"optional":null}"#;
    for segment in 1..=bytes.len() {
        let json = JsonReader::new(input(bytes, segment));
        let value: Request<'_> = json.decode().unwrap();
        assert_eq!(value.name, "alice");
        assert_eq!(value.escaped, "ab\n\"\\");
        assert_eq!(value.children[0].name, "你好");
        assert_eq!(value.children[1].name, "😀");
        assert!(value.active);
        assert_eq!(value.count, -42);
        assert_eq!(value.optional, None);
        assert_eq!(
            serde_json::to_value(&value).unwrap(),
            serde_json::from_slice::<serde_json::Value>(bytes).unwrap()
        );
    }
}

#[test]
fn contiguous_string_borrows_the_original_payload() {
    let bytes = br#"{"name":"alice"}"#;
    let mut source = input(bytes, 64);
    let original = source.fill_buf().unwrap().as_ptr();
    let json = JsonReader::new(source);
    let result: Child<'_> = json.decode().unwrap();
    assert_eq!(result.name.as_ptr(), original.wrapping_add(9));
}

#[tokio::test]
async fn borrowed_request_and_response_survive_an_async_handler() {
    let task = tokio::spawn(async move {
        let json = JsonReader::new(input(br#"{"name":"a\u0062"}"#, 1));
        let request: Child<'_> = json.decode().unwrap();
        tokio::task::yield_now().await;
        let response = Child { name: request.name };
        serde_json::to_string(&response).unwrap()
    });
    assert_eq!(task.await.unwrap(), r#"{"name":"ab"}"#);
}

#[derive(Debug, Deserialize, PartialEq)]
enum Action<'a> {
    Ping,
    Name(&'a str),
    Pair(u32, &'a str),
    Update { name: &'a str },
}

#[test]
fn enum_shapes_and_borrowed_values_are_supported() {
    let cases = [
        (r#""Ping""#, Action::Ping),
        (r#"{"Ping":null}"#, Action::Ping),
        (r#"{"Name":"a\u0062"}"#, Action::Name("ab")),
        (r#"{"Pair":[7,"value"]}"#, Action::Pair(7, "value")),
        (
            r#"{"Update":{"name":"value"}}"#,
            Action::Update { name: "value" },
        ),
    ];
    for (raw, expected) in cases {
        let json = JsonReader::new(input(raw.as_bytes(), 2));
        let value: Action<'_> = json.decode().unwrap();
        assert_eq!(value, expected);
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "kind")]
enum Tagged<'a> {
    Name { name: &'a str },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum Untagged<'a> {
    Name { name: &'a str },
    Number(i64),
}

#[test]
fn serde_tagging_flattening_and_ignored_fields_work() {
    let json = JsonReader::new(input(br#"{"kind":"Name","name":"a\u0062"}"#, 1));
    assert_eq!(
        json.decode::<Tagged<'_>>().unwrap(),
        Tagged::Name { name: "ab" }
    );
    let json = JsonReader::new(input(br#"{"name":"alice"}"#, 1));
    assert_eq!(
        json.decode::<Untagged<'_>>().unwrap(),
        Untagged::Name { name: "alice" }
    );
    #[derive(Deserialize)]
    struct Flat<'a> {
        #[serde(flatten, borrow)]
        child: Child<'a>,
    }
    let json = JsonReader::new(input(
        br#"{"name":"ab","ignored":{"deep":[null,true,"\u0061"]}}"#,
        1,
    ));
    assert_eq!(json.decode::<Flat<'_>>().unwrap().child.name, "ab");
}

#[test]
fn tuples_newtypes_units_bytes_chars_and_numeric_map_keys_work() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Newtype(u128);
    let json = JsonReader::new(input(
        br#"[340282366920938463463374607431768211455,null,"\uD83D\uDE00"]"#,
        3,
    ));
    let (number, (), character): (Newtype, (), char) = json.decode().unwrap();
    assert_eq!(number, Newtype(u128::MAX));
    assert_eq!(character, '😀');
    let json = JsonReader::new(input(br#""a\u0062""#, 1));
    assert_eq!(json.decode::<&[u8]>().unwrap(), b"ab");
    let json = JsonReader::new(input(br#"{"1":"a","-2":"b"}"#, 1));
    assert_eq!(
        json.decode::<BTreeMap<i32, &str>>().unwrap(),
        BTreeMap::from([(1, "a"), (-2, "b")])
    );
}

#[test]
fn malformed_documents_are_rejected_like_serde_json() {
    let invalid: &[&[u8]] = &[
        b"",
        b" ",
        b"[",
        b"{",
        b"[1,]",
        b"{\"a\":1,}",
        b"[1 2]",
        b"{\"a\" 1}",
        b"{1:2}",
        b"[true false]",
        b"null x",
        b"00",
        b"-01",
        b"+1",
        b"1.",
        b".1",
        b"1e+",
        b"1e9999",
        b"NaN",
        b"Infinity",
        b"\"abc",
        b"\"a\n\"",
        b"\"\xff\"",
        br#""\x""#,
        br#""\u12""#,
        br#""\uZZZZ""#,
        br#""\uD800""#,
        br#""\uDC00""#,
        br#""\uD800\u0000""#,
        br#""\uD800\uD800""#,
        br#""\uD800x""#,
        b"\"abc\\",
        b"\"\\\n\"",
    ];
    for &raw in invalid {
        assert!(
            serde_json::from_slice::<serde_json::Value>(raw).is_err(),
            "oracle accepted {raw:?}"
        );
        for segment in [1, 2, 7, 64] {
            let json = JsonReader::new(input(raw, segment));
            assert!(
                json.decode::<serde_json::Value>().is_err(),
                "accepted {raw:?} in segment {segment}"
            );
        }
    }
}

#[test]
fn nesting_limit_covers_unknown_fields_and_releases_the_depth_guard() {
    let json = JsonReader::with_recursion_limit(input(b"[[[0]]]", 1), 2);
    assert!(
        json.decode::<serde_json::Value>()
            .unwrap_err()
            .to_string()
            .contains("recursion limit")
    );
    let json = JsonReader::with_recursion_limit(input(b"[[0]]", 1), 2);
    assert_eq!(
        json.decode::<serde_json::Value>().unwrap(),
        serde_json::json!([[0]])
    );
    let json = JsonReader::with_recursion_limit(input(br#"{"name":"x","extra":[[[0]]]}"#, 1), 2);
    assert!(json.decode::<Child<'_>>().is_err());
}

#[test]
fn direct_serde_calls_consume_values_and_end_rejects_trailing_data() {
    let json = JsonReader::new(input(br#""first" "s\u0065cond""#, 2));
    let first = <&str>::deserialize(&json).unwrap();
    assert!(json.end().is_err());
    let second = <&str>::deserialize(&json).unwrap();
    json.end().unwrap();
    assert_eq!((first, second), ("first", "second"));
}

#[test]
fn generated_documents_match_serde_json_at_many_segment_sizes() {
    for size in 0..40 {
        let value = serde_json::json!({
            "text": "你好\n\t\"\\😀".repeat(size),
            "list": (0..size).map(|n| serde_json::json!({"n": n, "f": n as f64 / 7.0, "b": n % 2 == 0})).collect::<Vec<_>>(),
            "null": null, "max": u64::MAX, "min": i64::MIN,
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        let expected: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for segment in [1, 3, 11, 64] {
            let json = JsonReader::new(input(&bytes, segment));
            assert_eq!(json.decode::<serde_json::Value>().unwrap(), expected);
        }
    }
}

#[test]
fn numeric_keys_and_integer_overflow_match_serde_json() {
    for key in ["0", "-1", "01", " 1", "1 ", "+1", "1.0", "2147483648", ""] {
        let bytes = serde_json::to_vec(&BTreeMap::from([(key, "value")])).unwrap();
        let expected = serde_json::from_slice::<BTreeMap<i32, String>>(&bytes);
        let json = JsonReader::new(input(&bytes, 1));
        let actual = json.decode::<BTreeMap<i32, String>>();
        assert_eq!(actual.is_ok(), expected.is_ok(), "key {key:?}");
        if let (Ok(actual), Ok(expected)) = (actual, expected) {
            assert_eq!(actual, expected);
        }
    }
    for bytes in [b"127".as_slice(), b"128", b"-128", b"-129", b"1.0", b"1e0"] {
        let json = JsonReader::new(input(bytes, 1));
        assert_eq!(
            json.decode::<i8>().ok(),
            serde_json::from_slice::<i8>(bytes).ok()
        );
    }
}

#[test]
fn borrowed_parsers_have_independent_cursors_and_preserve_raw_input() {
    let raw = br#"{"name":"a\u0062"}"#;
    let source = input(raw, 1);
    let first = JsonReader::from_borrowed(&source);
    let second = JsonReader::from_borrowed(&source);
    #[derive(Deserialize)]
    struct Name<'a> {
        name: &'a str,
    }
    let one: Name<'_> = first.decode().unwrap();
    assert_eq!(source.position(), 0);
    assert_eq!(source.as_slice(), raw);
    // Advancing the shared source does not change either snapshot.
    source.skip(raw.len()).unwrap();
    let two: Name<'_> = second.decode().unwrap();
    assert_eq!((one.name, two.name), ("ab", "ab"));
    assert_eq!(first.position(), raw.len());
    assert_eq!(second.position(), raw.len());
}
