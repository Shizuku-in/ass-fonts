#![cfg(feature = "encoding")]

use ass_fonts::{decode_subtitle, read_subtitle};
use encoding_rs::{BIG5, GBK, SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE};

fn script(text: &str) -> String {
    format!(
        "[Script Info]\r\nTitle: 测试\r\n[V4+ Styles]\r\nFormat: Name, Fontname\r\nStyle: Default,{text}\r\n[Events]\r\nFormat: Layer, Start, End, Style, Text\r\nDialogue: 0,0:00:00.00,0:00:01.00,Default,{text}\r\n"
    )
}

#[test]
fn legacy_cjk_encodings_are_detected() {
    let samples = [
        (GBK, "微软雅黑，这是一段简体中文字幕。我们今天去看电影吧。"),
        (
            BIG5,
            "微軟正黑體，這是一段繁體中文字幕。我們今天去看電影吧。",
        ),
        (
            SHIFT_JIS,
            "ＭＳ ゴシック、これは日本語の字幕です。今日は映画を見に行きましょう。",
        ),
    ];
    for (encoding, text) in samples {
        let source = script(text).replace("测试", "");
        let (bytes, _, unmappable) = encoding.encode(&source);
        assert!(!unmappable, "{}", encoding.name());
        let decoded = decode_subtitle(&bytes).unwrap();
        assert_eq!(decoded.encoding, encoding);
        assert_eq!(decoded.text, source);
        assert!(!decoded.had_bom);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.ass");
        std::fs::write(&path, &bytes).unwrap();
        let fonts = read_subtitle(&path).unwrap();
        assert_eq!(fonts.references[0].name, text);
    }
}

#[test]
fn boms_and_utf8_take_precedence() {
    let source = script("宋体");
    let mut utf8 = b"\xef\xbb\xbf".to_vec();
    utf8.extend(source.as_bytes());
    let mut utf16le = vec![0xff, 0xfe];
    utf16le.extend(source.encode_utf16().flat_map(u16::to_le_bytes));
    let mut utf16be = vec![0xfe, 0xff];
    utf16be.extend(source.encode_utf16().flat_map(u16::to_be_bytes));
    for (bytes, encoding, bom) in [
        (source.as_bytes().to_vec(), UTF_8, false),
        (utf8, UTF_8, true),
        (utf16le, UTF_16LE, true),
        (utf16be, UTF_16BE, true),
    ] {
        let decoded = decode_subtitle(&bytes).unwrap();
        assert_eq!((decoded.encoding, decoded.had_bom), (encoding, bom));
        assert_eq!(decoded.text, source);
    }
    let error = decode_subtitle(&[0xff, 0xfe, 0]).unwrap_err();
    assert_eq!(error.encoding, UTF_16LE);
    assert!(decode_subtitle(b"\xef\xbb\xbf\xff").is_err());
}
