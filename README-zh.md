# ass-fonts

[English](README.md) | 简体中文

Rust 库：提取 ASS/SSA 对白实际使用的字体，扫描本地字体文件，并按字体内部名称匹配。

- 支持 ASS/SSA 样式、`\fn` 字体覆盖和 `\r` 样式重置，排除未使用样式、注释和绘图内容。
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

## 匹配规则与范围

按字体内部的家族名、全名、PostScript 名及相关名称匹配，统一执行 Unicode NFKC、转小写、空白规范化，并去除 ASS 竖排字体的 `@` 前缀。匹配不使用文件名。多个 face 命中时保留歧义，包括同一家族的 Regular/Bold 等变体。

`read_subtitle` 支持 UTF-8 和带 BOM 的 UTF-16 LE/BE。传统编码字幕请先解码，再传入 `extract_fonts`。

本库分析字体名称，不模拟渲染，不检查字形覆盖、不选择粗体/斜体变体、不处理字体回退、不提取内嵌字体，也不自动发现系统字体目录。请显式提供字体路径，并检查诊断以了解结果是否完整。

## 开发

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```
