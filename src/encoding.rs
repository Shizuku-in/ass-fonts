use encoding_rs::{Encoding, UTF_8};
use std::{error::Error, fmt};

/// Subtitle text decoded by [`decode_subtitle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedSubtitle {
    /// Decoded text without the BOM.
    pub text: String,
    /// Encoding of the original bytes.
    ///
    /// To write text back in the same encoding, encode with
    /// [`Encoding::output_encoding`]: `encoding_rs` cannot encode UTF-16 and
    /// substitutes UTF-8.
    pub encoding: &'static Encoding,
    /// Whether the original bytes started with a UTF-8 or UTF-16 BOM.
    pub had_bom: bool,
}

/// Bytes that are malformed in the detected or BOM-declared encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DecodeError {
    /// Encoding that was tried.
    pub encoding: &'static Encoding,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "subtitle is not valid {}", self.encoding.name())
    }
}

impl Error for DecodeError {}

/// Decode subtitle bytes, detecting their encoding.
///
/// 1. A UTF-8, UTF-16LE, or UTF-16BE BOM determines the encoding.
/// 2. Otherwise, valid UTF-8 is decoded as UTF-8.
/// 3. Otherwise, [`chardetng`] (the detector used by Firefox) guesses a legacy
///    encoding, such as GBK, Big5, Shift_JIS, EUC-KR, or windows-1252.
///
/// Decoding is strict: no replacement characters are introduced, so text
/// rewritten from the result can be re-encoded without silent damage.
///
/// # Errors
///
/// Returns [`DecodeError`] if the bytes are malformed in the chosen encoding,
/// including an odd UTF-16 byte count.
///
/// ```
/// // "字幕" in GBK.
/// let decoded = ass_fonts::decode_subtitle(b"Dialogue: \xd7\xd6\xc4\xbb").unwrap();
/// assert_eq!(decoded.text, "Dialogue: 字幕");
/// assert_eq!(decoded.encoding, encoding_rs::GBK);
/// ```
pub fn decode_subtitle(bytes: &[u8]) -> Result<DecodedSubtitle, DecodeError> {
    let (encoding, body, had_bom) = match Encoding::for_bom(bytes) {
        Some((encoding, length)) => (encoding, &bytes[length..], true),
        None if std::str::from_utf8(bytes).is_ok() => (UTF_8, bytes, false),
        None => {
            let mut detector = chardetng::EncodingDetector::new();
            detector.feed(bytes, true);
            (detector.guess(None, false), bytes, false)
        }
    };
    let text = encoding
        .decode_without_bom_handling_and_without_replacement(body)
        .ok_or(DecodeError { encoding })?;
    Ok(DecodedSubtitle {
        text: text.into_owned(),
        encoding,
        had_bom,
    })
}
