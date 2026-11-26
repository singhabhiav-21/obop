//! Which syntax a document is, read from its root element alone: the namespace URI and the local
//! name, never the prefix (UBL files use `ubl:`, `inv:` or a default namespace, CII files `rsm:`).
//!
//! Everything a reader gets passes through [`read`] first, so the limits hold for every syntax:
//! - at most [`MAX_INPUT_BYTES`] bytes and [`MAX_NODES`] nodes;
//! - UTF-8 only (XRechnung and Peppol BIS require it); a UTF-8 byte order mark is accepted;
//! - no DTD at all, so no entity expansion and no external entity is ever resolved (`roxmltree`
//!   refuses a DTD unless asked, and nothing here asks).

use std::fmt;

/// The largest document read. EN 16931 lets an invoice carry attachments (BG-24) inside the XML,
/// so this is well above an invoice without them; the largest KoSIT business case is 435 KB.
pub const MAX_INPUT_BYTES: usize = 20 * 1024 * 1024;

/// The most XML nodes (elements, attributes, text) read.
pub const MAX_NODES: u32 = 1_000_000;

const UBL_INVOICE: &str = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2";
const UBL_CREDIT_NOTE: &str = "urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2";
const CII: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";

/// The syntax of an e-invoice, as its root element says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Syntax {
    /// UBL 2.1 `Invoice` (XRechnung-UBL, Peppol BIS Billing 3.0).
    UblInvoice,
    /// UBL 2.1 `CreditNote`.
    UblCreditNote,
    /// UN/CEFACT Cross Industry Invoice D16B (XRechnung-CII, ZUGFeRD / Factur-X); a credit note is a
    /// CII invoice with type code 381.
    Cii,
}

impl Syntax {
    /// The namespace URI of the root element.
    pub fn namespace(self) -> &'static str {
        match self {
            Self::UblInvoice => UBL_INVOICE,
            Self::UblCreditNote => UBL_CREDIT_NOTE,
            Self::Cii => CII,
        }
    }

    /// The local name of the root element.
    pub fn root(self) -> &'static str {
        match self {
            Self::UblInvoice => "Invoice",
            Self::UblCreditNote => "CreditNote",
            Self::Cii => "CrossIndustryInvoice",
        }
    }

    /// The syntax in words, for messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::UblInvoice => "a UBL invoice",
            Self::UblCreditNote => "a UBL credit note",
            Self::Cii => "a CII invoice",
        }
    }

    fn of_root(namespace: Option<&str>, name: &str) -> Option<Self> {
        [Self::UblInvoice, Self::UblCreditNote, Self::Cii]
            .into_iter()
            .find(|s| namespace == Some(s.namespace()) && name == s.root())
    }
}

/// Why a document is not an e-invoice this crate reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectError {
    /// Larger than [`MAX_INPUT_BYTES`].
    TooLarge { bytes: usize },
    /// More XML nodes than [`MAX_NODES`].
    TooManyNodes,
    /// Not UTF-8.
    NotUtf8,
    /// Declares a DTD, which is never read.
    Dtd,
    /// Not well-formed XML (or empty); the text is the parser's message with its position.
    NotXml(String),
    /// Well-formed XML whose root is not an e-invoice.
    NotAnInvoice {
        namespace: Option<String>,
        name: String,
    },
}

impl fmt::Display for DetectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes } => write!(
                f,
                "the document has {bytes} bytes; at most {MAX_INPUT_BYTES} are read"
            ),
            Self::TooManyNodes => write!(f, "the document has more than {MAX_NODES} XML nodes"),
            Self::NotUtf8 => write!(f, "the document is not UTF-8"),
            Self::Dtd => write!(f, "the document declares a DTD, which is not read"),
            Self::NotXml(why) => write!(f, "not well-formed XML: {why}"),
            Self::NotAnInvoice { namespace, name } => match namespace {
                Some(ns) => write!(f, "not an e-invoice: the root element is {{{ns}}}{name}"),
                None => write!(
                    f,
                    "not an e-invoice: the root element is {name}, with no namespace"
                ),
            },
        }
    }
}

impl std::error::Error for DetectError {}

/// Parse a document under the limits above. Every reader starts here.
pub(crate) fn read(bytes: &[u8]) -> Result<roxmltree::Document<'_>, DetectError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(DetectError::TooLarge { bytes: bytes.len() });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| DetectError::NotUtf8)?;
    let options = roxmltree::ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_NODES,
        entity_resolver: None,
    };
    roxmltree::Document::parse_with_options(text, options).map_err(|e| match e {
        roxmltree::Error::DtdDetected => DetectError::Dtd,
        roxmltree::Error::NodesLimitReached => DetectError::TooManyNodes,
        other => DetectError::NotXml(other.to_string()),
    })
}

/// The syntax of an already parsed document.
pub(crate) fn syntax_of(doc: &roxmltree::Document<'_>) -> Result<Syntax, DetectError> {
    let root = doc.root_element().tag_name();
    Syntax::of_root(root.namespace(), root.name()).ok_or_else(|| DetectError::NotAnInvoice {
        namespace: root.namespace().map(str::to_string),
        name: root.name().to_string(),
    })
}

/// The syntax of a document, or why it is not an e-invoice this crate reads.
pub fn detect(bytes: &[u8]) -> Result<Syntax, DetectError> {
    syntax_of(&read(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;

    fn doc(body: &str) -> Vec<u8> {
        format!("{DECL}\n{body}").into_bytes()
    }

    #[test]
    fn the_root_decides_whatever_the_prefix() {
        let ubl = [
            format!(r#"<Invoice xmlns="{UBL_INVOICE}"/>"#),
            format!(r#"<ubl:Invoice xmlns:ubl="{UBL_INVOICE}"/>"#),
            format!(r#"<inv:Invoice xmlns:inv="{UBL_INVOICE}"/>"#),
        ];
        for d in &ubl {
            assert_eq!(detect(&doc(d)), Ok(Syntax::UblInvoice), "{d}");
        }
        assert_eq!(
            detect(&doc(&format!(r#"<CreditNote xmlns="{UBL_CREDIT_NOTE}"/>"#))),
            Ok(Syntax::UblCreditNote)
        );
        assert_eq!(
            detect(&doc(&format!(
                r#"<rsm:CrossIndustryInvoice xmlns:rsm="{CII}"/>"#
            ))),
            Ok(Syntax::Cii)
        );
    }

    #[test]
    fn the_right_name_in_the_wrong_namespace_is_not_an_invoice() {
        let cases = [
            ("<Invoice/>", None, "Invoice"),
            (
                r#"<Invoice xmlns="urn:example:not-ubl"/>"#,
                Some("urn:example:not-ubl"),
                "Invoice",
            ),
            // The UBL namespace of the invoice on the credit note's root, and the reverse.
            (
                &*format!(r#"<CreditNote xmlns="{UBL_INVOICE}"/>"#),
                Some(UBL_INVOICE),
                "CreditNote",
            ),
            (
                &*format!(r#"<Invoice xmlns="{CII}"/>"#),
                Some(CII),
                "Invoice",
            ),
            ("<order><id>1</id></order>", None, "order"),
        ];
        for (body, ns, name) in cases {
            assert_eq!(
                detect(&doc(body)),
                Err(DetectError::NotAnInvoice {
                    namespace: ns.map(str::to_string),
                    name: name.to_string()
                }),
                "{body}"
            );
        }
    }

    #[test]
    fn a_dtd_is_refused_before_any_entity_is_expanded() {
        let laughs = r#"<!DOCTYPE lolz [
  <!ENTITY lol "lol">
  <!ENTITY lol1 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
  <!ENTITY lol2 "&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;">
]>
<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2">&lol2;</Invoice>"#;
        let external = r#"<!DOCTYPE Invoice [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2">&xxe;</Invoice>"#;
        let bare = r#"<!DOCTYPE Invoice SYSTEM "http://example.invalid/invoice.dtd">
<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2"/>"#;
        for body in [laughs, external, bare] {
            assert_eq!(detect(&doc(body)), Err(DetectError::Dtd), "{body}");
        }
    }

    #[test]
    fn broken_or_foreign_input_is_refused_with_a_reason() {
        assert!(matches!(detect(b""), Err(DetectError::NotXml(_))));
        assert!(matches!(
            detect(&doc(&format!(r#"<Invoice xmlns="{UBL_INVOICE}"><cbc:ID>"#))),
            Err(DetectError::NotXml(_))
        ));
        assert_eq!(detect(&[0x3c, 0xff, 0xfe, 0x3e]), Err(DetectError::NotUtf8));
        // UTF-16 is not read. With its byte order mark, as a UTF-16 XML file starts, it is not
        // UTF-8; without one its bytes happen to be valid UTF-8 (`<` then NUL) and it is not XML.
        let utf16: Vec<u8> = "\u{feff}<a/>"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(detect(&utf16), Err(DetectError::NotUtf8));
        assert!(matches!(detect(&utf16[2..]), Err(DetectError::NotXml(_))));
        let e = detect(&doc("<a>")).unwrap_err();
        assert!(e.to_string().starts_with("not well-formed XML: "), "{e}");
    }

    #[test]
    fn a_utf8_byte_order_mark_is_accepted() {
        let mut with_bom = vec![0xef, 0xbb, 0xbf];
        with_bom.extend(doc(&format!(r#"<Invoice xmlns="{UBL_INVOICE}"/>"#)));
        assert_eq!(detect(&with_bom), Ok(Syntax::UblInvoice));
    }

    #[test]
    fn the_limits_hold_before_parsing() {
        let mut big = doc(&format!(r#"<Invoice xmlns="{UBL_INVOICE}">"#));
        big.resize(MAX_INPUT_BYTES + 1, b' ');
        assert_eq!(
            detect(&big),
            Err(DetectError::TooLarge {
                bytes: MAX_INPUT_BYTES + 1
            })
        );
        let many = doc(&format!(
            r#"<Invoice xmlns="{UBL_INVOICE}">{}</Invoice>"#,
            "<a/>".repeat(MAX_NODES as usize)
        ));
        assert!(many.len() <= MAX_INPUT_BYTES);
        assert_eq!(detect(&many), Err(DetectError::TooManyNodes));
    }
}
