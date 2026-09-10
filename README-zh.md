# ass-fonts

[English](README.md) | 简体中文

Rust 库：提取 ASS/SSA 对白实际使用的字体，扫描本地字体文件，并按字体内部名称匹配。

要求 Rust **1.87+**，采用 [MIT 许可证](LICENSE)。

- 跟踪 ASS/SSA 样式及 `\fn`、`\b`、`\i`、`\r`，排除未使用样式、注释和绘图内容。
- 记录每个字体引用实际使用的去重字符及对应字幕行。
- 扫描 TTF、OTF、TTC、OTC，包括字体集合中的全部 face。
- 返回 `resolved`（唯一候选）、`missing`（无候选）或 `ambiguous`（多个候选），附带引用行号、文件路径和 face 索引。
- 可选输出名义 cmap 覆盖结果：`complete`、`incomplete` 或 `uncheckable`。
- 提供解析与扫描诊断，报告支持序列化。

## 使用

```rust
use ass_fonts::{read_subtitle, FontIndex, ScanReport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subtitle = read_subtitle("movie.ass")?;
    let scan = ScanReport::scan(["./fonts"]);
    let index = FontIndex::new(scan.faces);
    let report = index.resolve(&subtitle.references);
    let coverage = report.check_coverage();

    println!("{report:#?}");
    println!("{coverage:#?}");
    // 检查诊断，了解被跳过或不完整的输入。
    eprintln!("{:?}", subtitle.diagnostics);
    eprintln!("{:?}", scan.issues);
    Ok(())
}
```

运行 JSON 报告示例：

```sh
cargo run --example report -- movie.ass ./fonts
cargo run --example report -- --check-coverage movie.ass ./fonts
```

`check_coverage` 会重新读取每个已选字体文件，检查分配给该引用的去重字符。歧义匹配按候选分别检查，因此候选 A 可以完整而候选 B 缺字。没有候选，或之后无法读取、解析的文件/face 会进入 `uncheckable`。这里只检查 Unicode cmap 的名义映射，不执行文本塑形和渲染，不应用跨家族回退，也不验证变体序列及视觉质量。

使用 `coverage.summary()` 获取候选与缺字计数，使用 `coverage.is_complete()` 执行严格成功判断，使用 `coverage.missing_characters()` 获取合并来源行后的全局去重缺字。`CharacterUsage::codepoint()` 可生成 `U+5B57` 形式的码点。

紧凑 JSON 与 CI 检查示例：

```sh
cargo run --example report -- --summary movie.ass ./fonts
cargo run --example report -- --summary \
  --fail-on-missing-font --fail-on-ambiguous-font \
  --fail-on-missing-glyph --fail-on-uncheckable movie.ass ./fonts
```

`--summary` 会自动检查覆盖，并输出带码点及行号的聚合缺字。失败策略默认关闭：参数错误退出码为 2；启用相应策略后，字体匹配失败为 3、缺字为 4、覆盖不可检查为 5；其他运行错误为 1。多个条件同时发生时，优先级依次为字体匹配、缺字、不可检查。

收集字体时，可允许由渲染器合成样式（粗体和斜体）：

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        synthesis: ass_fonts::SynthesisPolicy { bold: true, italic: true },
        ..Default::default()
    },
);
```

```sh
cargo run --example report -- --allow-style-synthesis movie.ass ./fonts
```

`ResolveOptions` 将字重选择（默认 `WeightMatching::Exact`，可选 `Nearest`）与合成能力（`bold`、`italic`，默认均关闭）分开。`resolve` 使用默认选项，`resolve_with_mode` 保留为兼容入口。先在原生倾斜状态中依次尝试精确字重、可选字重择近、允许的 400→700 粗体回退；仍无候选时，斜体权限允许斜体请求在直立字体中重复字重选择。直立请求不会反向回退到斜体。多个合适候选仍返回歧义。`matches[].synthetic_bold` 和 `synthetic_italic` 分别标记需要的合成。CLI `--allow-style-synthesis` 和 `ResolveMode::AllowStyleSynthesis` 同时允许两种合成；使用 `SynthesisPolicy` 可单独开启其中一种。本库不生成字体文件，也未验证实际渲染效果。


字重择近可独立开启，不自动允许样式合成：

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        weight_matching: ass_fonts::WeightMatching::Nearest,
        ..Default::default()
    },
);
```

```sh
cargo run --example report -- --nearest-weight movie.ass ./fonts
cargo run --example report -- --nearest-weight --allow-style-synthesis movie.ass ./fonts
```

`Nearest` 优先精确匹配，否则在同一家族、相同斜体状态下选择字重绝对差值最小者。等距离及重复 face 保留歧义，不设距离上限；具体名称的定位规则不变。选择字体后独立判断合成：700→693 标记 `family_nearest`，不标记合成粗体；700→400 仅在显式允许合成时标记合成粗体。JSON 示例现在输出完整 `options` 对象，替代原来的 `mode` 字段。

每个条目包含 `candidates` 和 `matches`（每个候选对应一条匹配依据）。缺失条目的候选及匹配依据为空，并附带：

| `missing_reason` | 含义 | `available_variants` |
| --- | --- | --- |
| `name_not_found` | 没有匹配的内部名称 | 省略 |
| `style_not_found` | 家族存在，但没有所需字重/斜体 | 扫描到的该家族全部 face |

`matches[].matched_names` 保留命中的原始内部名称及 `kind`：`family`、`full_name` 或 `post_script_name`。调用方提供的名称若没有类型信息，标为 `internal_name`。已有变体仅供参考，不作为回退选择。成功和歧义条目省略 `missing_reason`。

扫描名称保留原始 `name_id`。每个匹配记录包含 `selection_method`：`post_script_name`、`full_name`、`legacy_family_name`、`family_exact`、`family_nearest`、`family_synthesis` 或 `internal_name`。

例如，JSON 报告中的名称缺失条目：

```json
{
  "reference": {
    "name": "Unknown Font", "weight": 400, "italic": false,
    "lines": [12], "characters": [{ "character": "A", "lines": [12] }]
  },
  "candidates": [],
  "missing_reason": "name_not_found",
  "matches": []
}
```

## 匹配规则与范围

按字体内部的家族名、全名、PostScript 名及相关名称匹配，统一执行 Unicode NFKC、转小写、空白规范化，并去除 ASS 竖排字体的 `@` 前缀。匹配不使用文件名。

引用按名称、字重和斜体状态分组。普通体/粗体对应 400/700，保留 `\b100`–`\b900` 指定的字重。PostScript 名优先；完整名称用于定位具体 face，但与通用家族名重合时仍按家族处理。传统家族别名具有更广的排印家族记录、且命中的 face 字重/斜体属性一致时，也可定位具体变体。判断依据是名称表关系，不解析 Bold、W17 等后缀。

通用家族按配置选择字重，并优先匹配斜体属性（oblique 也视为斜体）；元数据不足时保守地按家族处理。具体名称保留字体的原生设计，不因请求为 400 而拒绝匹配；这表示字体依赖已定位，不保证请求的视觉样式已满足。报告保留请求与实际属性，同名多个文件仍返回歧义。斜体合成需要显式允许。

`read_subtitle` 支持 UTF-8 和带 BOM 的 UTF-16 LE/BE。传统编码字幕请先解码，再传入 `extract_fonts`。

本库可以检查名义 cmap 覆盖，但不模拟文本塑形/渲染、不生成合成字体、不实例化可变字体、不处理跨家族字体回退、不提取内嵌字体，也不自动发现系统字体目录。粗体合成仅支持 400→700，斜体合成支持直立→斜体；两种权限可组合，不修改字体文件。粗体合成不会使用 Light 字体。请显式提供字体路径，并检查诊断以了解结果是否完整。

## 开发

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

CI 平台矩阵、最低 Rust 版本验证与发布包检查见[发布准备说明](RELEASING.md)；API 和 JSON 行为说明见 [CHANGELOG.md](CHANGELOG.md)。本地 `real-test` 字体和字幕不会进入 crate 发布包。
