# ass-fonts

[English](README.md) | 简体中文

提取 ASS/SSA 对白实际使用的字体，扫描本地字体文件，并按字体内部名称匹配的 Rust 库。

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

允许**由渲染器合成样式**（粗体和斜体）：

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        synthesis: ass_fonts::SynthesisPolicy { bold: true, italic: true },
        ..Default::default()
    },
);
```

运行**字重择近**：

```rust
let report = index.resolve_with_options(
    &subtitle.references,
    ass_fonts::ResolveOptions {
        weight_matching: ass_fonts::WeightMatching::Nearest,
        ..Default::default()
    },
);
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
cargo bench --locked --bench pipeline
```

基准使用运行时生成、可再分发的测试数据。字体扫描测量的是操作系统文件缓存预热后的性能，不代表冷磁盘读取速度。

字幕解析与字体匹配的模糊测试见[模糊测试说明](FUZZING.md)；CI 平台矩阵、最低 Rust 版本验证与发布包检查见[发布准备说明](RELEASING.md)；API 和 JSON 行为说明见 [CHANGELOG.md](CHANGELOG.md)。本地 `real-test` 字体和字幕不会进入 crate 发布包。

# 许可证

[MIT](./LICENSE)