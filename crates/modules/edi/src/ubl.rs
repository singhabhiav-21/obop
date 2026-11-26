//! The UBL 2.1 reader: XRechnung-UBL and Peppol BIS Billing 3.0, both `Invoice` and `CreditNote`.
//!
//! So far it reads the document header only: [`read_header`] gives a [`Header`] with the business
//! terms below. [`DocFormat::parse`] needs a whole [`Invoice`], which requires the seller, the buyer
//! and the totals, so until the reader has them it reads the header (refusing it as `read_header`
//! does) and then answers [`ParseError::Incomplete`] rather than make up the rest.
//!
//! | Term  | Invoice                         | CreditNote                                    |
//! |-------|---------------------------------|-----------------------------------------------|
//! | BT-1  | `cbc:ID`                        | `cbc:ID`                                      |
//! | BT-2  | `cbc:IssueDate`                 | `cbc:IssueDate`                               |
//! | BT-3  | `cbc:InvoiceTypeCode`           | `cbc:CreditNoteTypeCode`                      |
//! | BT-5  | `cbc:DocumentCurrencyCode`      | `cbc:DocumentCurrencyCode`                    |
//! | BT-9  | `cbc:DueDate`                   | `cac:PaymentMeans/cbc:PaymentDueDate`         |
//! | BT-10 | `cbc:BuyerReference`            | `cbc:BuyerReference`                          |
//! | BT-24 | `cbc:CustomizationID`           | `cbc:CustomizationID`                         |
//! | BG-3  | `cac:BillingReference/cac:InvoiceDocumentReference` (BT-25 `cbc:ID`, BT-26 `cbc:IssueDate`) | the same |
//!
//! Every element is matched by namespace URI and local name, never by prefix. The header terms are
//! direct children of the root. A missing required term (BT-1, BT-2, BT-3, BT-5, BT-24) is
//! [`ParseError::Missing`]; a term that appears twice, a date not written `YYYY-MM-DD` or a currency
//! not shaped like ISO 4217 is [`ParseError::Invalid`]. Values are taken as written, never trimmed.

use crate::detect::{Syntax, read, syntax_of};
use crate::format::{BuildError, DocFormat, Finding, ParseError, Report, Severity};
use crate::model::{Currency, Date, Header, Invoice, PrecedingInvoice, date_from_iso};
use roxmltree::Node;

const CBC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2";
const CAC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2";

/// UBL 2.1 as EN 16931 uses it (CEN/TS 16931-3-2), for XRechnung-UBL and Peppol BIS Billing 3.0.
#[derive(Debug, Clone, Copy, Default)]
pub struct UblFormat;

impl DocFormat for UblFormat {
    fn id(&self) -> &'static str {
        "ubl"
    }

    fn version(&self) -> &'static str {
        "2.1"
    }

    /// Reads and checks the header, then refuses with [`ParseError::Incomplete`]: the reader does
    /// not read the parties (BG-4, BG-7) and the totals (BG-22) yet, which an [`Invoice`] requires.
    fn parse(&self, bytes: &[u8]) -> Result<Invoice, ParseError> {
        read_header(bytes)?;
        Err(ParseError::Incomplete {
            format: self.id(),
            missing: "BG-4, BG-7 and BG-22",
        })
    }

    /// Only whether the header can be read so far: an error of [`read_header`] is the one fatal
    /// finding. The EN 16931, XRechnung and Peppol rules are not checked yet (fork issue #7), which
    /// the warning says, so an empty report never claims a document is valid.
    fn validate(&self, bytes: &[u8]) -> Report {
        let mut findings = Vec::new();
        if let Err(e) = read_header(bytes) {
            let location = match &e {
                ParseError::Missing { term } | ParseError::Invalid { term, .. } => {
                    Some((*term).to_string())
                }
                _ => None,
            };
            findings.push(Finding {
                rule: "OBOP-UBL-READ".into(),
                severity: Severity::Fatal,
                message: e.to_string(),
                location,
            });
        }
        findings.push(Finding {
            rule: "OBOP-RULES-NOT-CHECKED".into(),
            severity: Severity::Warning,
            message: "the EN 16931, XRechnung and Peppol rules are not checked yet".into(),
            location: None,
        });
        Report { findings }
    }

    fn build(&self, _invoice: &Invoice) -> Result<Vec<u8>, BuildError> {
        Err(BuildError::Unsupported(
            "writing UBL is not built yet (fork issue #9)".into(),
        ))
    }
}

/// The header of a UBL `Invoice` or `CreditNote`, read under the limits of [`crate::detect`]. A CII
/// document is [`ParseError::WrongSyntax`].
pub fn read_header(bytes: &[u8]) -> Result<Header, ParseError> {
    let doc = read(bytes)?;
    let syntax = syntax_of(&doc)?;
    match syntax {
        Syntax::UblInvoice | Syntax::UblCreditNote => header(doc.root_element(), syntax),
        Syntax::Cii => Err(ParseError::WrongSyntax {
            expected: Syntax::UblInvoice,
            found: syntax,
        }),
    }
}

/// The header terms of an `Invoice` or `CreditNote` root.
fn header(root: Node<'_, '_>, syntax: Syntax) -> Result<Header, ParseError> {
    let credit_note = syntax == Syntax::UblCreditNote;
    let type_code_name = if credit_note {
        "CreditNoteTypeCode"
    } else {
        "InvoiceTypeCode"
    };

    let number = required(root, CBC, "ID", "BT-1")?.to_string();
    let issue_date = date(required(root, CBC, "IssueDate", "BT-2")?, "BT-2")?;
    let type_code = required(root, CBC, type_code_name, "BT-3")?.to_string();
    let currency = currency(required(root, CBC, "DocumentCurrencyCode", "BT-5")?, "BT-5")?;
    let due_date = if credit_note {
        credit_note_due_date(root)?
    } else {
        optional(root, CBC, "DueDate", "BT-9")?
            .map(|d| date(d, "BT-9"))
            .transpose()?
    };
    let buyer_reference = optional(root, CBC, "BuyerReference", "BT-10")?.map(str::to_string);
    let specification_id = required(root, CBC, "CustomizationID", "BT-24")?.to_string();
    let preceding = preceding(root)?;

    Ok(Header {
        number,
        issue_date,
        type_code,
        currency,
        due_date,
        buyer_reference,
        specification_id,
        preceding,
    })
}

/// BT-9 of a credit note: UBL's `CreditNote` has no `cbc:DueDate`, so EN 16931 puts it on the
/// payment means. Several payment means may each carry it; they must agree.
fn credit_note_due_date(root: Node<'_, '_>) -> Result<Option<Date>, ParseError> {
    let mut found: Option<(&str, Date)> = None;
    for means in children(root, CAC, "PaymentMeans") {
        if let Some(text) = optional(means, CBC, "PaymentDueDate", "BT-9")? {
            let d = date(text, "BT-9")?;
            if let Some((_, first)) = found
                && first != d
            {
                return Err(ParseError::Invalid {
                    term: "BT-9",
                    value: text.to_string(),
                    reason: format!("differs from the payment due date {first} given earlier"),
                });
            }
            found = Some((text, d));
        }
    }
    Ok(found.map(|(_, d)| d))
}

/// BG-3: each `cac:BillingReference` that holds an `cac:InvoiceDocumentReference`. A billing
/// reference to another kind of document is not BG-3 and is skipped.
fn preceding(root: Node<'_, '_>) -> Result<Vec<PrecedingInvoice>, ParseError> {
    let mut out = Vec::new();
    for billing in children(root, CAC, "BillingReference") {
        let Some(reference) = single(billing, CAC, "InvoiceDocumentReference", "BG-3")? else {
            continue;
        };
        let number = required(reference, CBC, "ID", "BT-25")?.to_string();
        let issue_date = optional(reference, CBC, "IssueDate", "BT-26")?
            .map(|d| date(d, "BT-26"))
            .transpose()?;
        out.push(PrecedingInvoice { number, issue_date });
    }
    Ok(out)
}

fn children<'a, 'i>(
    parent: Node<'a, 'i>,
    ns: &'static str,
    name: &'static str,
) -> impl Iterator<Item = Node<'a, 'i>> {
    parent.children().filter(move |n| {
        n.is_element() && n.tag_name().namespace() == Some(ns) && n.tag_name().name() == name
    })
}

/// The one child element with this name, if any; a second one is refused.
fn single<'a, 'i>(
    parent: Node<'a, 'i>,
    ns: &'static str,
    name: &'static str,
    term: &'static str,
) -> Result<Option<Node<'a, 'i>>, ParseError> {
    let mut it = children(parent, ns, name);
    let first = it.next();
    if let Some(second) = it.next() {
        return Err(ParseError::Invalid {
            term,
            value: text_of(second).unwrap_or_default().to_string(),
            reason: format!("{name} appears more than once"),
        });
    }
    Ok(first)
}

/// The text of a single-valued term; an element with no text counts as absent.
fn optional<'a>(
    parent: Node<'a, '_>,
    ns: &'static str,
    name: &'static str,
    term: &'static str,
) -> Result<Option<&'a str>, ParseError> {
    Ok(single(parent, ns, name, term)?.and_then(text_of))
}

fn required<'a>(
    parent: Node<'a, '_>,
    ns: &'static str,
    name: &'static str,
    term: &'static str,
) -> Result<&'a str, ParseError> {
    optional(parent, ns, name, term)?.ok_or(ParseError::Missing { term })
}

/// The element's text, or `None` when it has none or only whitespace.
fn text_of<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.text().filter(|t| !t.trim().is_empty())
}

fn date(text: &str, term: &'static str) -> Result<Date, ParseError> {
    date_from_iso(text).ok_or_else(|| ParseError::Invalid {
        term,
        value: text.to_string(),
        reason: "not a calendar date written YYYY-MM-DD".into(),
    })
}

fn currency(text: &str, term: &'static str) -> Result<Currency, ParseError> {
    Currency::new(text).ok_or_else(|| ParseError::Invalid {
        term,
        value: text.to_string(),
        reason: "not an ISO 4217 alphabetic code".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVOICE: &str = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2";
    const CREDIT_NOTE: &str = "urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2";
    const SPEC: &str = "urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0";

    /// A UBL document with the given root, prefix for the root and body.
    fn ubl(root: &str, ns: &str, prefix: &str, body: &str) -> Vec<u8> {
        let (open, close, xmlns) = if prefix.is_empty() {
            (
                root.to_string(),
                root.to_string(),
                format!(r#"xmlns="{ns}""#),
            )
        } else {
            (
                format!("{prefix}:{root}"),
                format!("{prefix}:{root}"),
                format!(r#"xmlns:{prefix}="{ns}""#),
            )
        };
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<{open} {xmlns} xmlns:c="{CBC}" xmlns:a="{CAC}">{body}</{close}>"#
        )
        .into_bytes()
    }

    fn invoice_body(without: &str) -> String {
        [
            (
                "BT-24",
                format!("<c:CustomizationID>{SPEC}</c:CustomizationID>"),
            ),
            ("BT-1", "<c:ID>F-1</c:ID>".to_string()),
            ("BT-2", "<c:IssueDate>2026-11-20</c:IssueDate>".to_string()),
            ("BT-9", "<c:DueDate>2026-12-20</c:DueDate>".to_string()),
            (
                "BT-3",
                "<c:InvoiceTypeCode>380</c:InvoiceTypeCode>".to_string(),
            ),
            (
                "BT-5",
                "<c:DocumentCurrencyCode>SEK</c:DocumentCurrencyCode>".to_string(),
            ),
            (
                "BT-10",
                "<c:BuyerReference>Anna</c:BuyerReference>".to_string(),
            ),
        ]
        .into_iter()
        .filter(|(t, _)| *t != without)
        .map(|(_, e)| e)
        .collect()
    }

    fn day(s: &str) -> Date {
        date_from_iso(s).unwrap()
    }

    #[test]
    fn an_invoice_header_is_read() {
        let inv = read_header(&ubl("Invoice", INVOICE, "ubl", &invoice_body(""))).unwrap();
        assert_eq!(inv.number, "F-1");
        assert_eq!(inv.issue_date, day("2026-11-20"));
        assert_eq!(inv.type_code, "380");
        assert_eq!(inv.currency.as_str(), "SEK");
        assert_eq!(inv.due_date, Some(day("2026-12-20")));
        assert_eq!(inv.buyer_reference.as_deref(), Some("Anna"));
        assert_eq!(inv.specification_id, SPEC);
        assert!(inv.preceding.is_empty());
    }

    #[test]
    fn parse_reads_the_header_and_then_says_the_reader_is_incomplete() {
        let got = UblFormat.parse(&ubl("Invoice", INVOICE, "", &invoice_body("")));
        let e = got.unwrap_err();
        assert_eq!(
            e,
            ParseError::Incomplete {
                format: "ubl",
                missing: "BG-4, BG-7 and BG-22"
            }
        );
        assert_eq!(
            e.to_string(),
            "the ubl reader is incomplete: it does not read BG-4, BG-7 and BG-22 yet"
        );
        // A broken header is refused for what is wrong with it, not as incomplete.
        assert_eq!(
            UblFormat.parse(&ubl("Invoice", INVOICE, "", &invoice_body("BT-1"))),
            Err(ParseError::Missing { term: "BT-1" })
        );
    }

    #[test]
    fn the_prefix_does_not_matter() {
        let body = invoice_body("");
        let a = read_header(&ubl("Invoice", INVOICE, "", &body)).unwrap();
        let b = read_header(&ubl("Invoice", INVOICE, "inv", &body)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn a_missing_required_term_is_named() {
        for term in ["BT-1", "BT-2", "BT-3", "BT-5", "BT-24"] {
            let got = read_header(&ubl("Invoice", INVOICE, "", &invoice_body(term)));
            assert_eq!(got, Err(ParseError::Missing { term }), "{term}");
        }
        let empty = invoice_body("BT-1") + "<c:ID>  </c:ID>";
        assert_eq!(
            read_header(&ubl("Invoice", INVOICE, "", &empty)),
            Err(ParseError::Missing { term: "BT-1" })
        );
        let e = read_header(&ubl("Invoice", INVOICE, "", &invoice_body("BT-1"))).unwrap_err();
        assert_eq!(e.to_string(), "BT-1 is missing");
    }

    #[test]
    fn missing_optional_terms_are_none() {
        let body = invoice_body("BT-9").replace("<c:BuyerReference>Anna</c:BuyerReference>", "");
        let inv = read_header(&ubl("Invoice", INVOICE, "", &body)).unwrap();
        assert_eq!(inv.due_date, None);
        assert_eq!(inv.buyer_reference, None);
        assert!(inv.preceding.is_empty());
    }

    #[test]
    fn a_bad_date_is_refused_with_its_term() {
        for (term, bad) in [
            ("BT-2", "2026-02-30"),
            ("BT-2", "2026-11-20T10:00:00"),
            ("BT-2", "20261120"),
            ("BT-9", " 2026-12-20"),
        ] {
            let body = if term == "BT-2" {
                invoice_body("BT-2") + &format!("<c:IssueDate>{bad}</c:IssueDate>")
            } else {
                invoice_body("BT-9") + &format!("<c:DueDate>{bad}</c:DueDate>")
            };
            match read_header(&ubl("Invoice", INVOICE, "", &body)) {
                Err(ParseError::Invalid { term: t, value, .. }) => {
                    assert_eq!((t, value.as_str()), (term, bad));
                }
                other => panic!("{term} {bad:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_bad_currency_and_a_repeated_term_are_refused() {
        let body = invoice_body("BT-5") + "<c:DocumentCurrencyCode>sek</c:DocumentCurrencyCode>";
        assert!(matches!(
            read_header(&ubl("Invoice", INVOICE, "", &body)),
            Err(ParseError::Invalid { term: "BT-5", .. })
        ));
        let twice = invoice_body("") + "<c:ID>F-2</c:ID>";
        assert!(matches!(
            read_header(&ubl("Invoice", INVOICE, "", &twice)),
            Err(ParseError::Invalid { term: "BT-1", .. })
        ));
    }

    #[test]
    fn a_credit_note_reads_its_type_code_due_date_and_preceding_invoices() {
        let body = format!(
            "<c:CustomizationID>{SPEC}</c:CustomizationID>
             <c:ID>CN-7</c:ID>
             <c:IssueDate>2026-11-25</c:IssueDate>
             <c:CreditNoteTypeCode>381</c:CreditNoteTypeCode>
             <c:DocumentCurrencyCode>EUR</c:DocumentCurrencyCode>
             <a:BillingReference><a:InvoiceDocumentReference>
               <c:ID>F-1</c:ID><c:IssueDate>2026-11-20</c:IssueDate>
             </a:InvoiceDocumentReference></a:BillingReference>
             <a:BillingReference><a:InvoiceDocumentReference>
               <c:ID>F-0</c:ID>
             </a:InvoiceDocumentReference></a:BillingReference>
             <a:BillingReference><a:CreditNoteDocumentReference>
               <c:ID>CN-1</c:ID>
             </a:CreditNoteDocumentReference></a:BillingReference>
             <a:PaymentMeans><c:PaymentMeansCode>30</c:PaymentMeansCode>
               <c:PaymentDueDate>2026-12-25</c:PaymentDueDate></a:PaymentMeans>"
        );
        let cn = read_header(&ubl("CreditNote", CREDIT_NOTE, "", &body)).unwrap();
        assert_eq!(cn.number, "CN-7");
        assert_eq!(cn.type_code, "381");
        assert_eq!(cn.due_date, Some(day("2026-12-25")));
        assert_eq!(
            cn.preceding,
            vec![
                PrecedingInvoice {
                    number: "F-1".into(),
                    issue_date: Some(day("2026-11-20")),
                },
                PrecedingInvoice {
                    number: "F-0".into(),
                    issue_date: None,
                },
            ]
        );

        // An invoice type code on a credit note is not its BT-3.
        let wrong = body.replace("CreditNoteTypeCode", "InvoiceTypeCode");
        assert_eq!(
            read_header(&ubl("CreditNote", CREDIT_NOTE, "", &wrong)),
            Err(ParseError::Missing { term: "BT-3" })
        );
        // BG-3 without its number names BT-25.
        let no_number = body.replace("<c:ID>F-0</c:ID>", "");
        assert_eq!(
            read_header(&ubl("CreditNote", CREDIT_NOTE, "", &no_number)),
            Err(ParseError::Missing { term: "BT-25" })
        );
        // Two payment means with different due dates.
        let split = body.replace(
            "</a:PaymentMeans>",
            "</a:PaymentMeans><a:PaymentMeans><c:PaymentMeansCode>30</c:PaymentMeansCode>
               <c:PaymentDueDate>2026-12-26</c:PaymentDueDate></a:PaymentMeans>",
        );
        assert!(matches!(
            read_header(&ubl("CreditNote", CREDIT_NOTE, "", &split)),
            Err(ParseError::Invalid { term: "BT-9", .. })
        ));
    }

    #[test]
    fn header_terms_in_the_wrong_namespace_do_not_count() {
        let body = invoice_body("BT-1") + r#"<x:ID xmlns:x="urn:example:other">F-1</x:ID>"#;
        assert_eq!(
            read_header(&ubl("Invoice", INVOICE, "", &body)),
            Err(ParseError::Missing { term: "BT-1" })
        );
    }

    #[test]
    fn cii_is_the_wrong_syntax() {
        let cii = br#"<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100"/>"#;
        assert_eq!(
            read_header(cii),
            Err(ParseError::WrongSyntax {
                expected: Syntax::UblInvoice,
                found: Syntax::Cii
            })
        );
    }

    #[test]
    fn validate_never_calls_a_document_valid_yet_and_build_says_it_is_not_built() {
        let ok = UblFormat.validate(&ubl("Invoice", INVOICE, "", &invoice_body("")));
        assert!(ok.is_valid());
        assert_eq!(ok.findings.len(), 1);
        assert_eq!(ok.findings[0].severity, Severity::Warning);
        let bad = UblFormat.validate(&ubl("Invoice", INVOICE, "", &invoice_body("BT-1")));
        assert!(!bad.is_valid());
        assert_eq!(bad.findings[0].location.as_deref(), Some("BT-1"));
        assert!(matches!(
            UblFormat.build(&an_invoice()),
            Err(BuildError::Unsupported(_))
        ));
    }

    /// A whole invoice, written by hand, as the reader cannot make one yet.
    fn an_invoice() -> Invoice {
        use crate::model::{Address, CountryCode, Money, Party, Totals};
        let sek = Currency::new("SEK").unwrap();
        let money = |minor| Money {
            minor,
            currency: sek,
        };
        let party = |name: &str| Party {
            name: name.into(),
            trading_name: None,
            identifiers: Vec::new(),
            legal_registration: None,
            vat_id: None,
            tax_registration: None,
            endpoint: None,
            address: Address {
                lines: Vec::new(),
                city: None,
                postal_code: None,
                subdivision: None,
                country: CountryCode::new("SE").unwrap(),
            },
        };
        Invoice {
            number: "F-1".into(),
            issue_date: day("2026-11-20"),
            type_code: "380".into(),
            currency: sek,
            vat_accounting_currency: None,
            due_date: None,
            buyer_reference: None,
            payment_terms: None,
            business_process: None,
            specification_id: SPEC.into(),
            preceding: Vec::new(),
            seller: party("S"),
            buyer: party("B"),
            lines: Vec::new(),
            vat_breakdown: Vec::new(),
            totals: Totals {
                line_total: money(0),
                allowances: None,
                charges: None,
                tax_exclusive: money(0),
                vat_total: None,
                vat_total_accounting: None,
                tax_inclusive: money(0),
                prepaid: None,
                rounding: None,
                payable: money(0),
            },
        }
    }
}
