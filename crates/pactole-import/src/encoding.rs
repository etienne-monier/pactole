//! Tolerant decoding of raw OFX source bytes into UTF-8 text.
//!
//! OFX documents declare their encoding either in the legacy SGML header
//! (`ENCODING:` and `CHARSET:` lines, OFX 1.x) or in the XML declaration
//! (`<?xml … encoding="…"?>`, OFX 2.x). Real-world headers are unreliable:
//! banks routinely declare `USASCII` (or UTF-8) while emitting bytes from a
//! legacy code page such as windows-1252. The decoder therefore follows an
//! ordered, never-failing chain: honour a usable declaration, otherwise try
//! UTF-8 strictly, and finally fall back to windows-1252 (a superset of
//! ISO-8859-1 that cannot fail on any byte sequence).

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, WINDOWS_1252};

/// Number of leading bytes inspected when looking for an encoding
/// declaration: the OFX header and the XML declaration always sit at the
/// very beginning of the document, well within this range.
const PROBE_LEN: usize = 1024;

/// The encoding declaration inferred from a document's preamble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclaredEncoding {
    /// `ENCODING:UNICODE` (UTF-16 little endian body following the
    /// plain-text header; a file *with* a BOM is handled before detection
    /// ever runs).
    Utf16Le,
    /// A declared encoding that [`encoding_rs`] can decode directly.
    Known(&'static Encoding),
    /// No usable declaration: the generic chain (UTF-8, then windows-1252)
    /// applies.
    Unspecified,
}

/// Decodes raw OFX source bytes into UTF-8 text.
///
/// The declared encoding is honoured when one can be determined: the
/// `<?xml … encoding="…"?>` declaration of an OFX 2.x document, the
/// `ENCODING:`/`CHARSET:` lines of an OFX 1.x SGML header (Windows code
/// pages 1250–1258, 874, the CJK code pages 932/936/949/950, Mac Roman),
/// `ENCODING:UNICODE` (a UTF-16LE body after the plain-text header), or a
/// leading byte order mark.
///
/// Otherwise — absent or unknown declaration, `USASCII`/UTF-8 declaration
/// (which constrains nothing a fallback cannot handle), or bytes that are
/// not valid UTF-8 despite the declaration — the decoder falls back to
/// windows-1252. Every byte sequence is decodable, so this function never
/// fails: a mislabeled file is decoded as best it can be rather than
/// rejected, and an initial `U+FEFF` character is always stripped.
///
/// # Examples
///
/// ```
/// use pactole_import::decode_ofx_source;
///
/// let bytes = [0x43, 0x61, 0x66, 0xE9]; // "Café" in windows-1252
/// assert_eq!(decode_ofx_source(&bytes), "Café");
/// ```
pub fn decode_ofx_source(bytes: &[u8]) -> String {
    // A byte order mark takes precedence over any declaration.
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return finalize(UTF_16LE.decode(rest).0.into_owned());
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return finalize(UTF_16BE.decode(rest).0.into_owned());
    }
    // A UTF-8 BOM is stripped up front so that the rest of the chain below
    // (and any legacy decoding) never reinterprets it as content.
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);

    let text = match declared_encoding(bytes) {
        DeclaredEncoding::Utf16Le => decode_declared_utf16_le(bytes),
        DeclaredEncoding::Known(encoding) => encoding.decode(bytes).0.into_owned(),
        DeclaredEncoding::Unspecified => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => WINDOWS_1252.decode(bytes).0.into_owned(),
        },
    };

    finalize(text)
}

/// Decodes a document whose header declares `ENCODING:UNICODE`.
///
/// The declaration itself lives in the plain-text header, so only the body
/// that follows it (everything from the first `<` on) is UTF-16LE; the
/// header is kept as-is.
fn decode_declared_utf16_le(bytes: &[u8]) -> String {
    let body_start = bytes.iter().position(|&byte| byte == b'<').unwrap_or(0);
    let (header, body) = bytes.split_at(body_start);
    let mut text = String::from_utf8_lossy(header).into_owned();
    text.push_str(&UTF_16LE.decode(body).0);
    text
}

/// Strips a residual leading BOM character, if any.
fn finalize(text: String) -> String {
    match text.strip_prefix('\u{FEFF}') {
        Some(rest) => rest.to_owned(),
        None => text,
    }
}

/// Infers the declared encoding from the document preamble.
fn declared_encoding(bytes: &[u8]) -> DeclaredEncoding {
    let probe = String::from_utf8_lossy(&bytes[..bytes.len().min(PROBE_LEN)]);

    if let Some(label) = xml_declaration_encoding(&probe) {
        return label_encoding(label);
    }

    sgml_header_encoding(&probe)
}

/// Extracts the `encoding` value of an `<?xml …?>` declaration, when the
/// document starts with one and it carries an encoding.
fn xml_declaration_encoding(probe: &str) -> Option<&str> {
    let head = probe.trim_start();
    if !head.starts_with("<?xml") {
        return None;
    }
    let end = head.find("?>")?;
    let declaration = &head[..end];
    let lower = declaration.to_ascii_lowercase();
    let index = lower.find("encoding=")?;
    let rest = &declaration[index + "encoding=".len()..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value = &rest[quote.len_utf8()..];
    let end_quote = value.find(quote)?;
    value.get(..end_quote)
}

/// Reads the `ENCODING:` and `CHARSET:` lines of a legacy SGML header.
///
/// The header stops at the first `<`, where the OFX body begins.
fn sgml_header_encoding(probe: &str) -> DeclaredEncoding {
    let header_end = probe.find('<').unwrap_or(probe.len());
    let header = &probe[..header_end];

    let mut encoding: Option<&str> = None;
    let mut charset: Option<&str> = None;
    for line in header.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim().to_ascii_uppercase().as_str() {
            "ENCODING" => encoding = Some(value.trim()),
            "CHARSET" => charset = Some(value.trim()),
            _ => {}
        }
    }

    // The OFX header spells these values in several ways; normalise them.
    match encoding.map(str::to_ascii_uppercase).as_deref() {
        Some("USASCII") | Some("US-ASCII") => charset_encoding(charset),
        Some("UTF-8") | Some("UTF8") => DeclaredEncoding::Unspecified,
        Some("ISO8859-1") | Some("ISO88591") | Some("ISO-8859-1") => label_encoding("iso-8859-1"),
        Some("UNICODE") => DeclaredEncoding::Utf16Le,
        // Absent or unknown `ENCODING:`: a known `CHARSET` still says what
        // the bytes are; otherwise the generic chain applies.
        _ => charset_encoding(charset),
    }
}

/// Maps an OFX `CHARSET` value — a Windows code page number in most
/// headers — to an encoding.
fn charset_encoding(charset: Option<&str>) -> DeclaredEncoding {
    let Some(charset) = charset else {
        return DeclaredEncoding::Unspecified;
    };
    let charset = charset.trim();
    if charset.is_empty() || charset.eq_ignore_ascii_case("NONE") {
        return DeclaredEncoding::Unspecified;
    }

    let label = match charset {
        "874" => "windows-874".to_string(),
        "932" => "shift_jis".to_string(),
        "936" => "gbk".to_string(),
        "949" => "euc-kr".to_string(),
        "950" => "big5".to_string(),
        "10000" => "macintosh".to_string(),
        other => match other.parse::<u16>() {
            // Windows code pages 1250–1258 are spelled as plain numbers in
            // OFX headers, which is not a WHATWG label on its own.
            Ok(codepage) if (1250..=1258).contains(&codepage) => format!("windows-{codepage}"),
            // Spelled-out labels (e.g. "windows-1252") reach `label_encoding`
            // as-is; unknown values fall through to the generic chain there.
            _ => return label_encoding(other),
        },
    };

    label_encoding(&label)
}

/// Resolves a WHATWG encoding label to a declared encoding.
///
/// UTF-8 and ASCII labels are treated as *unspecified*: they carry no
/// constraint the generic chain does not already handle, and honouring them
/// directly would reject (or worse, misdecode) banks that mislabel their
/// bytes — the most common real-world case.
fn label_encoding(label: &str) -> DeclaredEncoding {
    let label = label.trim().to_ascii_lowercase();

    if matches!(
        label.as_str(),
        "utf-8" | "utf8" | "ascii" | "us-ascii" | "ansi_x3.4-1968"
    ) {
        return DeclaredEncoding::Unspecified;
    }

    match Encoding::for_label(label.as_bytes()) {
        Some(encoding) => DeclaredEncoding::Known(encoding),
        None => DeclaredEncoding::Unspecified,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_ofx_source;

    /// Legacy SGML header declaring windows-1252, as written by most banks.
    const SGML_CP1252: &str = "OFXHEADER:100\n\
        DATA:OFXSGML\n\
        VERSION:102\n\
        SECURITY:NONE\n\
        ENCODING:USASCII\n\
        CHARSET:1252\n\
        COMPRESSION:NONE\n\
        OLDFILEUID:NONE\n\
        NEWFILEUID:NONE\n\
        \n";

    /// Builds `header + body`, where `body` is raw (possibly non-UTF-8) bytes.
    fn document(header: &str, body: &[u8]) -> Vec<u8> {
        let mut bytes = header.as_bytes().to_vec();
        bytes.extend_from_slice(body);
        bytes
    }

    /// `"<NAME>Caf"` + `é` (0xE9) + `"</NAME>"` in windows-1252.
    fn cp1252_e_acute_tail() -> &'static [u8] {
        b"<NAME>Caf\xE9</NAME>"
    }

    #[test]
    fn passes_valid_utf8_through_unchanged() {
        assert_eq!(
            decode_ofx_source(b"<OFX>Caf\xC3\xA9</OFX>"),
            "<OFX>Café</OFX>"
        );
    }

    #[test]
    fn strips_a_utf8_byte_order_mark() {
        assert_eq!(
            decode_ofx_source(b"\xEF\xBB\xBF<OFX>ok</OFX>"),
            "<OFX>ok</OFX>"
        );
    }

    #[test]
    fn decodes_a_utf16_byte_order_mark() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "é".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_ofx_source(&bytes), "é");
    }

    #[test]
    fn decodes_the_code_page_declared_in_the_sgml_header() {
        let source = document(SGML_CP1252, cp1252_e_acute_tail());
        assert_eq!(
            decode_ofx_source(&source),
            format!("{}<NAME>Café</NAME>", SGML_CP1252)
        );
    }

    #[test]
    fn falls_back_to_windows_1252_when_the_declaration_lies() {
        // Declared UTF-8 but sent as windows-1252 (common bank sloppiness).
        let header = "OFXHEADER:100\nENCODING:UTF-8\nCHARSET:NONE\n\n";
        let source = document(header, cp1252_e_acute_tail());
        assert_eq!(
            decode_ofx_source(&source),
            format!("{header}<NAME>Café</NAME>")
        );
    }

    #[test]
    fn falls_back_when_no_declaration_is_present() {
        // windows-1252 bytes, no header at all: not valid UTF-8.
        assert_eq!(
            decode_ofx_source(cp1252_e_acute_tail()),
            "<NAME>Café</NAME>"
        );
        // Valid UTF-8 without a header stays UTF-8.
        assert_eq!(
            decode_ofx_source(b"<NAME>Caf\xC3\xA9</NAME>"),
            "<NAME>Café</NAME>"
        );
    }

    #[test]
    fn ignores_an_unknown_charset() {
        let header = "OFXHEADER:100\nENCODING:USASCII\nCHARSET:ZZZZ9\n\n";
        let source = document(header, cp1252_e_acute_tail());
        assert_eq!(
            decode_ofx_source(&source),
            format!("{header}<NAME>Café</NAME>")
        );
    }

    #[test]
    fn honours_the_xml_declaration() {
        let declaration = "<?xml version=\"1.0\" encoding=\"windows-1252\"?>\n";
        let source = document(declaration, cp1252_e_acute_tail());
        assert_eq!(
            decode_ofx_source(&source),
            format!("{declaration}<NAME>Café</NAME>")
        );
    }

    #[test]
    fn decodes_the_cjk_code_pages() {
        // SHIFT_JIS: 0x82 0xA0 is あ.
        let header = "OFXHEADER:100\nENCODING:USASCII\nCHARSET:932\n\n";
        let source = document(header, &[0x82, 0xA0]);
        assert_eq!(decode_ofx_source(&source), format!("{header}あ"));
    }

    #[test]
    fn decodes_utf16_without_a_byte_order_mark_when_declared() {
        // Plain-text header, UTF-16LE body: only the body is decoded as
        // UTF-16; the header is kept as-is.
        let header = "OFXHEADER:100\nENCODING:UNICODE\nCHARSET:NONE\n\n";
        let mut source = header.as_bytes().to_vec();
        for unit in "<OFX>OK</OFX>".encode_utf16() {
            source.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_ofx_source(&source), format!("{header}<OFX>OK</OFX>"));
    }
}
