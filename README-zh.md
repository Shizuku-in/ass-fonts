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

## 匹配规则与范围

- 按字体内部名称匹配，忽略大小写、空白和竖排 `@` 前缀，不使用文件名。
- 指向具体 face 的名称保留其原生字重/斜体；家族名按请求的字重和斜体选择。
- 粗体（400→700）和斜体合成默认关闭，需通过 `SynthesisPolicy` 允许。
- `read_subtitle` 支持 UTF-8 和带 BOM 的 UTF-16；其他编码请先自行解码，再传入 `extract_fonts`。

## 开发

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo bench --locked --bench pipeline
```

- 模糊测试：[FUZZING.md](FUZZING.md)
- 版本变更记录：[CHANGELOG.md](CHANGELOG.md)

# 许可证

[MIT](./LICENSE)