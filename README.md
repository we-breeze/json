# brz-json

基于 `brz-io::Reader` 的 JSON 反序列化器。包名为 `brz-json`，依赖别名 `brz-json`，
Rust 中使用 `brz_json`。通过 `&JsonReader` 实现 `serde::Deserializer<'de>`，业务
struct 继续使用 `#[derive(Deserialize)]`，包括 `name: &'a str` 这样的借用字段。

## 使用

首次发布成功后，从 crates.io 引入：

```toml
[dependencies]
brz-json = "0.0.2"
brz-io = "0.0.2"
brz-ds = { package = "brz-ds", version = "0.0.2", default-features = false }
serde = { version = "1", features = ["derive"] }
```

```rust
use brz_ds::EphemeralBytesArena;
use brz_io::Writer;
use brz_json::JsonReader;
use serde::Deserialize;
use std::io::Write;

#[derive(Deserialize)]
struct Params<'a> {
    name: &'a str,
}

let arena = EphemeralBytesArena::new(3); // Small chunks demonstrate cross-segment strings.
let mut writer = Writer::new(&arena);
writer.write_all(br#"{"name":"a\u0062"}"#)?;
let json = JsonReader::new(writer.into_reader());
let params: Params<'_> = json.decode()?;
assert_eq!(params.name, "ab");
```

流接收仍使用标准同步或 Tokio IO：先通过 `copy` 将有上限的输入写入 Writer，再转为
Reader。JSON 解析是同步内存操作。保持 JsonReader 存活，解析后的借用 struct 就可以
交给异步 handler，直到 handler 和借用返回值的序列化完成。

已有 Reader 可以通过 `JsonReader::from_borrowed(&reader)` 创建解析器。它对当前
未读范围建立借用视图，拥有独立游标；解析不推进源 Reader，源 Reader 后续的共享
读取也不改变该解析器的输入范围。适用于 HTTP body 解析后仍需访问原始字节的场景。

## 存储与解析

- 普通字符串在单片内：直接引用原始 payload。
- 普通字符串跨片：Reader 只合并这个字符串的范围。
- 带转义字符串：直接解码到 Reader 的 arena，支持标准转义和 Unicode 代理对。
- 数组、对象等由 Serde visitor 直接构建目标类型，解析器不先构建 `serde_json::Value`。
- 数字 token 的语法、范围和浮点转换委托给 serde_json，避免重复实现数值转换。

字段名也会通过相同的字符串解析路径。所有共享读取产生的引用都由 Reader 保存，
消费游标不会提前释放仍可能被借用的数据。JsonReader 是 Send，但不是 Sync，解析
游标由一个调用者顺序使用。多个 handler 不应并发解析同一个 JsonReader。

`decode` 读取一个完整文档并检查尾部。也可以使用 `T::deserialize(&json)` 逐个消费
独立 JSON 值，再调用 `json.end()` 检查是否结束。解析会推进游标，失败可能已消费
部分输入，失败后应丢弃该解析器。错误包含可用的原始输入字节偏移。

默认最大嵌套深度为 128，可通过 `with_recursion_limit` 调整。支持常用 Serde 类型，
包括借用字符串和字节、Option、数组/元组、struct、map、枚举及 Serde 的常用派生属性。
嵌套借用字段仍按 Serde 规则使用 `#[serde(borrow)]`。

这是独立反序列化器，不实现 serde_json 的私有 `RawValue` / `arbitrary_precision`
协议。JSON 文本要求有效 UTF-8；不支持 serde_json 字节接口的非 UTF-8/WTF-8 扩展。
自定义 visitor 和 Serde 派生属性可能自行分配中间结构；这里保证的是解析器不引入
统一的 Value 中间树。未知字段仍会被完整校验，其字符串可能经过相同的 arena 存储路径。

本项目使用 crates.io 的 `brz-io 0.0.2`，关闭其默认 Tokio 特性。
HTTP server 的 API 宏直接使用此解析器。

## 验证

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo test --doc
cargo clippy --all-targets -- -D warnings
```

测试覆盖一字节分片、片内指针复用、跨片与转义字段同时借用、异步 handler 生命周期、
代理对、嵌套结构、枚举、非法输入、深度限制，以及与 serde_json 的对照结果。

## CI and publishing

Pushes and pull requests run rustfmt, Clippy with warnings denied, integration
and documentation tests, and the same tests in release mode. Cargo.lock is
tracked. This crate has no feature variants or Loom models and forbids unsafe
code.

The default branch is `main`. Grant this public repository access to the
`we-breeze` organization Actions secret `CARGO_REGISTRY_TOKEN`. The token must
allow creating and publishing `brz-json`; repository rules must allow Actions
to push version commits and create tags (`contents: write`).

Use **Actions → Publish → Run workflow**, select `main`, and leave `retry_tag`
empty. Publish increments the latest `v0.0.x` tag, updates Cargo.toml and
Cargo.lock, runs checks and a publishing dry run, then atomically pushes the
version commit and annotated tag before uploading to crates.io. The existing
`v0.0.1` means the next release is `v0.0.2`, replacing the initial unpublished
Cargo version `0.1.0`. Pushes and merges only run CI; publishing is manual.

If upload fails after the tag is pushed, start a new run with `retry_tag` set
to that existing tag. This field does not choose a new version. Check crates.io
before retrying an upload timeout: published versions cannot be overwritten.
Publishing is serialized and rejects stale checkouts. Source fixes require a
new release. No GitHub Release is created.

## Operational safety

Bound untrusted input with `brz_io::Writer::with_limit` and an input byte limit.
The default recursion limit is 128; increasing it for untrusted input can
exhaust the thread stack. The limit does not cap document size, string length,
array length, or allocations made by custom Serde visitors. Cross-segment and
escaped strings, including ignored fields, can retain additional arena memory.
Discard failed parsers and bound repeated parsing of the same borrowed reader.

## License

Licensed under either the MIT license or the Apache License, Version 2.0,
at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
