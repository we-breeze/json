# brz-json

统一 JSON 编解码、字段状态和显式兼容策略。模型继续使用 Serde 的
`Serialize` / `Deserialize`，业务代码通过 `brz_json` 使用 JSON 能力。

```toml
[dependencies]
brz-json = "0.0.7"
serde = { version = "1", features = ["derive"] }
```

## 字段缺失与 null

普通 `Option<T>` 合并缺失和 `null`。只有需要区分二者的更新请求或兼容模型使用宏：

```rust
use brz_json::FieldState;
use serde::{Deserialize, Serialize};

#[brz_json::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Update {
    #[json(presence)]
    title: String,
    // 只有历史协议允许错误类型时才开启 lenient。
    #[json(presence, lenient)]
    legacy_name: String,
}

let update: Update = brz_json::from_str(r#"{"title":null}"#)?;
assert!(update.title.is_null());
assert!(update.legacy_name.is_missing());
assert_eq!(brz_json::to_string(&update)?, r#"{"title":null}"#);
# Ok::<(), brz_json::Error>(())
```

业务只声明实际字段类型，不直接声明宏的内部 `Field<T>`。`FieldState` 提供
`is_missing`、`is_null`、`is_present`、`value`、`into_value`。严格模式的非空错误类型
报错；`lenient` 将其记为“已提供、非 null、没有有效值”。缺失字段序列化时省略，
显式 null 保留。可用 `.into()`、`set`、`set_null`、`unset` 构造或更新字段。

`#[brz_json::model]` 放在 derive 前面。普通结构体无需这个宏。
`rename`、借用和泛型仍遵循 Serde 规则；presence 字段自动设置 missing 默认值和
省略规则，不支持自定义 `default = ...` 或 `skip_serializing_if`。

## 选择合适的 JSON 类型

| 需求 | 类型 / 入口 |
| --- | --- |
| 已知结构 | `struct`、`from_str` / `from_slice`、`to_string` / `to_vec` / `to_writer` |
| 原样透传，保留数字写法、空白、对象顺序 | `&RawJson` 或 `Box<RawJson>`、`to_raw_value` |
| 历史 Value 解析校验、规范化后冻结 | `OpaqueJson`；`decode::<T>()` 严格投影，`project::<T>()` 容错投影 |
| 需要容错的集合元素投影 | `JsonProjection<T>`；有效 JSON 不匹配 T 时 `value` 为 None |
| 保持插入顺序的动态键对象 | `Object<T>`；重复键取最后值，位置保留首次出现的位置 |
| 必须动态修改的无 schema 文档 | `Value`、`Map`、`json!`，限制在对应边界 |

`OpaqueJson` 反序列化沿用 Value 的有限数值校验，例如拒绝 `1e400`；`RawJson`
保留合法 JSON 数字文本，不要求转换成 f64。`OpaqueJson::from_json_text` 明确用于原始
文本捕获；其内容未必能转换为 Value，调用 `to_value` 前须保证数值可表示。
可失败的序列化使用 `try_from_serializable`，已知不会失败的负载可用 `from_serializable`。

## 显式兼容策略

使用 `#[serde(default, deserialize_with = "brz_json::adapters::...")]`。
`default` 控制缺失，adapter 控制已提供的值，二者独立。

- `null_as_default`：null 使用类型默认值，错误类型报错。
- `try_decode` / `invalid_as_default`：完整消费有效 JSON 后容忍类型不匹配；语法错误仍报错。
- `object` / `optional_object` / `optional_objects`：要求对象形状，拒绝 Serde 可能接受的位置数组。
- `nonempty_string`：null 或空字符串为 None；`nonnull_items`：列表只过滤 null，其他错误元素报错。
- `integer` / `number` / `boolean` 及对应 `optional_*`：显式字符串、布尔、数值转换。
  `integer` 只接受整数浮点；`truncated_integer` 明确允许浮点截断。
- `legacy_boolean`：历史响应策略，允许任意有符号整数、trim 后的布尔拼写和空字符串。
  与严格限定 0/1 的 `boolean`、Python 的 `PythonTruthiness` / `python_truthy` 分开使用。

业务枚举、默认值、权限、错误响应和特殊 Python 格式化留在业务项目。

## 分片输入与借用

`&JsonReader` 实现 `serde::Deserializer<'de>`。它直接构建目标类型，普通字符串
可借用输入；跨片字符串和转义字符串保存在 Reader 的 arena 中。保持 Reader 存活，
借用模型可以跨 await 使用。Reader 为 Send，非 Sync；失败后丢弃解析器。

```rust
use brz_io::{EphemeralBytesArena, Writer};
use brz_json::JsonReader;
use serde::Deserialize;
use std::io::Write;

#[derive(Deserialize)]
struct Params<'a> { name: &'a str }
let arena = EphemeralBytesArena::new(3);
let mut writer = Writer::new(&arena);
writer.write_all(br#"{"name":"a\u0062"}"#)?;
let json = JsonReader::new(writer.into_reader());
let params: Params<'_> = json.decode()?;
assert_eq!(params.name, "ab");
# Ok::<(), Box<dyn std::error::Error>>(())
```

`decode` 检查完整文档及尾部；逐值使用 `T::deserialize(&json)` 后调用 `end()`。
已有 Reader 可用 `from_borrowed` 创建独立游标，不推进源 Reader。
嵌套借用字段使用 `#[serde(borrow)]`。支持 RawJson 捕获，不支持 arbitrary_precision
和非 UTF-8 字节扩展。容错 untagged adapter 和 OpaqueJson 可以分配中间结构。

错误提供 `path()`，分片解析提供可用的 `byte_offset()`。输入应限制总字节数；
默认最大嵌套深度 128，可通过 `with_recursion_limit` 调整。

## 验证和发布

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace --release
```

GitHub Actions 的 Publish workflow 从 main 手动触发，递增 `v0.0.x`，同步两个包的版本，
验证 workspace 后原子推送版本提交和 tag，依次发布 `brz-json-macros` 与 `brz-json`。
发布凭据需要有两个包的发布权限。宏包先做 dry-run；主包在宏包发布后由 cargo publish
校验打包结果。部分发布失败可用已有 `retry_tag` 重试，已发布包会跳过。

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
