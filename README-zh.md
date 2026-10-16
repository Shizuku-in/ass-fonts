# ass-fonts

[English](README.md) | 简体中文

提取 ASS/SSA 对白实际使用的字体，扫描本地字体文件，并按字体内部名称匹配的 Rust 库。

- 跟踪 ASS/SSA 样式及 `\fn`、`\b`、`\i`、`\r`，排除未使用样式、注释和绘图内容。
- 记录每个字体引用实际使用的去重字符、空白字符及对应字幕行，以及是否通过 `@` 竖排引用。
- 用同一套解析逻辑改写字幕中的字体名，其余字节保持不变。
- 扫描 TTF、OTF、TTC、OTC，包括字体集合中的全部 face 和旧式中日韩编码的 name 记录。
- 支持扫描结果缓存（`discover_font_files`、`scan_font_file`）、进度回调和可选的并行扫描。
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
    // 检查诊断
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

**改写字体名**（例如封装子集字体前混淆名称）：

```rust
let rewritten = ass_fonts::rewrite_font_names(&text, |site| {
    // 返回 None 表示保留原名；`@` 前缀由库自动保留。
    Some(format!("{:08X}", hash(site.name)))
});
```

## Feature

| Feature | 默认 | 用途 |
| --- | --- | --- |
| `legacy-names` | 开启 | 解码 Windows/Mac 平台旧式中日韩编码的 name 记录 |
| `encoding` | 关闭 | `decode_subtitle`：检测 GBK、Big5、Shift_JIS 等旧编码字幕（基于 `chardetng`），`read_subtitle` 也会使用 |
| `parallel` | 关闭 | 用 rayon 并行扫描字体文件，结果与串行一致 |

## 匹配规则与范围

- 按字体内部名称匹配，忽略大小写、空白和竖排 `@` 前缀，不使用文件名。
- 指向具体 face 的名称保留其原生字重/斜体；家族名按请求的字重和斜体选择。
- 粗体（400→700）和斜体合成默认关闭，需通过 `SynthesisPolicy` 允许。
- 未开启 `encoding` 时，`read_subtitle` 支持 UTF-8 和带 BOM 的 UTF-16；其他编码请先自行解码，再传入 `extract_fonts`。
- 覆盖检查默认不检查空白字符，需要时设置 `CoverageOptions::include_whitespace`。

## 开发

```sh
cargo test --locked --all-features
cargo clippy --locked --all-targets -- -D warnings
cargo bench --locked --bench pipeline
```

- 模糊测试：[FUZZING.md](FUZZING.md)
- 版本变更记录：[CHANGELOG.md](CHANGELOG.md)

# 许可证

[MIT](./LICENSE)