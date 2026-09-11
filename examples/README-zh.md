# 示例

[English](README.md) | 简体中文

## JSON 报告示例

```sh
cargo run --example report -- movie.ass ./fonts
cargo run --example report -- --check-coverage movie.ass ./fonts
```

`check_coverage` 会重新读取每个已选字体文件，检查分配给该引用的去重字符。歧义匹配按候选分别检查，因此候选 A 可以完整而候选 B 缺字。没有候选，或之后无法读取、解析的文件/face 会进入 `uncheckable`。这里只检查 Unicode cmap 的名义映射，不执行文本塑形和渲染，不应用跨家族回退，也不验证变体序列及视觉质量。

使用 `coverage.summary()` 获取候选与缺字计数，使用 `coverage.is_complete()` 执行严格成功判断，使用 `coverage.missing_characters()` 获取合并来源行后的全局去重缺字。`CharacterUsage::codepoint()` 可生成 `U+5B57` 形式的码点。

## 紧凑 JSON 与 CI 检查示例

```sh
cargo run --example report -- --summary movie.ass ./fonts
cargo run --example report -- --summary \
  --fail-on-missing-font --fail-on-ambiguous-font \
  --fail-on-missing-glyph --fail-on-uncheckable movie.ass ./fonts
```

`--summary` 会自动检查覆盖，并输出带码点及行号的聚合缺字。失败策略默认关闭：参数错误退出码为 2；启用相应策略后，字体匹配失败为 3、缺字为 4、覆盖不可检查为 5；其他运行错误为 1。多个条件同时发生时，优先级依次为字体匹配、缺字、不可检查。

## 允许合成样式

```sh
cargo run --example report -- --allow-style-synthesis movie.ass ./fonts
```

`ResolveOptions` 将字重选择（默认 `WeightMatching::Exact`，可选 `Nearest`）与合成能力（`bold`、`italic`，默认均关闭）分开。`resolve` 使用默认选项，`resolve_with_mode` 保留为兼容入口。先在原生倾斜状态中依次尝试精确字重、可选字重择近、允许的 400→700 粗体回退；仍无候选时，斜体权限允许斜体请求在直立字体中重复字重选择。直立请求不会反向回退到斜体。多个合适候选仍返回歧义。`matches[].synthetic_bold` 和 `synthetic_italic` 分别标记需要的合成。CLI `--allow-style-synthesis` 和 `ResolveMode::AllowStyleSynthesis` 同时允许两种合成；使用 `SynthesisPolicy` 可单独开启其中一种。本库不生成字体文件，也未验证实际渲染效果。

## 字重择近

```sh
cargo run --example report -- --nearest-weight movie.ass ./fonts
cargo run --example report -- --nearest-weight --allow-style-synthesis movie.ass ./fonts
```

`Nearest` 优先精确匹配，否则在同一家族、相同斜体状态下选择字重绝对差值最小者。等距离及重复 face 保留歧义，不设距离上限；具体名称的定位规则不变。选择字体后独立判断合成：700→693 标记 `family_nearest`，不标记合成粗体；700→400 仅在显式允许合成时标记合成粗体。JSON 示例现在输出完整 `options` 对象，替代原来的 `mode` 字段。

## 含义

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