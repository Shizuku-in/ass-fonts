# ass-fonts

[English](README.md) | 简体中文

Rust 库：提取 ASS/SSA 对白实际使用的字体，扫描本地字体文件，并按字体内部名称匹配。

- 跟踪 ASS/SSA 样式及 `\fn`、`\b`、`\i`、`\r`，排除未使用样式、注释和绘图内容。
- 扫描 TTF、OTF、TTC、OTC，包括字体集合中的全部 face。
- 返回 `resolved`（唯一候选）、`missing`（无候选）或 `ambiguous`（多个候选），附带引用行号、文件路径和 face 索引。
- 提供解析与扫描诊断，报告支持序列化。

## 使用

```rust
use ass_fonts::{read_subtitle, FontIndex, ScanReport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subtitle = read_subtitle("movie.ass")?;
    let scan = ScanReport::scan(["./fonts"]);
    let index = FontIndex::new(scan.faces);
    let report = index.resolve(&subtitle.references);

    println!("{report:#?}");
    // 检查诊断，了解被跳过或不完整的输入。
    eprintln!("{:?}", subtitle.diagnostics);
    eprintln!("{:?}", scan.issues);
    Ok(())
}
```

运行 JSON 报告示例：

```sh
cargo run --example report -- movie.ass ./fonts
```

收集字体时，可允许由渲染器合成样式（目前仅支持粗体）：

```rust
let report = index.resolve_with_mode(
    &subtitle.references,
    ass_fonts::ResolveMode::AllowStyleSynthesis,
);
```

```sh
cargo run --example report -- --allow-style-synthesis movie.ass ./fonts
```

默认 `resolve` 仍为严格模式。可选模式优先选择精确匹配的家族变体，再允许斜体状态相同的 400 字重字体满足 700 字重请求。多个合适候选仍返回歧义。`matches[].synthetic_bold: true` 标记需要加粗的候选，包括适用此规则的具体名称匹配。本库不生成字体文件，也未验证实际渲染效果。

每个条目包含 `candidates` 和 `matches`（每个候选对应一条匹配依据）。缺失条目的候选及匹配依据为空，并附带：

| `missing_reason` | 含义 | `available_variants` |
| --- | --- | --- |
| `name_not_found` | 没有匹配的内部名称 | 省略 |
| `style_not_found` | 家族存在，但没有所需字重/斜体 | 扫描到的该家族全部 face |

`matches[].matched_names` 保留命中的原始内部名称及 `kind`：`family`、`full_name` 或 `post_script_name`。调用方提供的名称若没有类型信息，标为 `internal_name`。已有变体仅供参考，不作为回退选择。成功和歧义条目省略 `missing_reason`。

例如，JSON 报告中的名称缺失条目：

```json
{
  "reference": { "name": "Unknown Font", "weight": 400, "italic": false, "lines": [12] },
  "candidates": [],
  "missing_reason": "name_not_found",
  "matches": []
}
```

## 匹配规则与范围

按字体内部的家族名、全名、PostScript 名及相关名称匹配，统一执行 Unicode NFKC、转小写、空白规范化，并去除 ASS 竖排字体的 `@` 前缀。匹配不使用文件名。

引用按名称、字重和斜体状态分组。普通体/粗体对应 400/700，保留 `\b100`–`\b900` 指定的字重。家族名要求字重和斜体属性精确匹配（oblique 也视为斜体）；没有对应变体时返回 `missing`，多个匹配 face 仍返回 `ambiguous`。全名/PostScript 名用于定位具体字体，不按请求的样式筛选；名称同时也是家族别名时，优先按家族处理。

`read_subtitle` 支持 UTF-8 和带 BOM 的 UTF-16 LE/BE。传统编码字幕请先解码，再传入 `extract_fonts`。

本库不模拟渲染、不检查字形覆盖、不生成合成字体、不选择最近字重、不实例化可变字体、不处理跨家族字体回退、不提取内嵌字体，也不自动发现系统字体目录。可选粗体合成策略仅支持 400→700，不合成斜体，也不回退到 Light 字体。请显式提供字体路径，并检查诊断以了解结果是否完整。

## 开发

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```
