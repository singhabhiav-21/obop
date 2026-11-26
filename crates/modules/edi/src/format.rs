//! The syntax-independent face of an e-invoice format (docs/LOCALIZATION.md 2.3: `DocFormat { id,
//! version, build, parse, validate }`; docs/ARCHITECTURE.md, the E-invoicing row). XRechnung-UBL,
//! XRechnung-CII, Peppol BIS Billing 3.0 and ZUGFeRD / Factur-X each implement it; none does yet.
//! `EDocTransport` (send, poll, receive, cancel) comes with the first transport.

use crate::detect::{DetectError, Syntax};
use crate::model::Invoice;
use std::fmt;

/// One e-invoice format: a syntax with the rules of one specification.
pub trait DocFormat {
    /// A stable id, such as `xrechnung-ubl` or `peppol-bis-3`.
    fn id(&self) -> &'static str;

    /// The version of the specification implemented, such as `3.0.2`.
    fn version(&self) -> &'static str;

    /// Read a document into the model. A value that does not fit the model is refused, never
    /// rounded or guessed.
    fn parse(&self, bytes: &[u8]) -> Result<Invoice, ParseError>;

    /// Check a document against the format's rules and say what is wrong with it.
    fn validate(&self, bytes: &[u8]) -> Report;

    /// Write an invoice in this format.
    fn build(&self, invoice: &Invoice) -> Result<Vec<u8>, BuildError>;
}

/// Why a document could not be read into the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Not an e-invoice this crate can read at all.
    Detect(DetectError),
    /// An e-invoice, but in another syntax than the format reads.
    WrongSyntax { expected: Syntax, found: Syntax },
    /// A business term the model requires is missing, such as `BT-1`.
    Missing { term: &'static str },
    /// A business term holds a value the model cannot take, such as an amount with too many
    /// decimals for its currency.
    Invalid {
        term: &'static str,
        value: String,
        reason: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Detect(e) => e.fmt(f),
            Self::WrongSyntax { expected, found } => {
                write!(f, "expected {}, found {}", expected.name(), found.name())
            }
            Self::Missing { term } => write!(f, "{term} is missing"),
            Self::Invalid {
                term,
                value,
                reason,
            } => write!(f, "{term} {value:?}: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<DetectError> for ParseError {
    fn from(e: DetectError) -> Self {
        Self::Detect(e)
    }
}

/// Why an invoice could not be written in a format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// The format requires a business term the invoice does not have.
    Missing { term: &'static str },
    /// The invoice uses something the format cannot express.
    Unsupported(String),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { term } => write!(f, "{term} is missing"),
            Self::Unsupported(what) => write!(f, "not supported: {what}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// How bad a finding is: a fatal one makes the document invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Fatal,
}

/// One rule a document breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The rule's id, such as `BR-CO-15` or `PEPPOL-EN16931-R003`.
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    /// Where in the document, when known (an XPath or a business term).
    pub location: Option<String>,
}

/// The outcome of [`DocFormat::validate`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    /// Valid when no finding is fatal; warnings do not make a document invalid.
    pub fn is_valid(&self) -> bool {
        self.findings.iter().all(|f| f.severity != Severity::Fatal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_is_invalid_only_with_a_fatal_finding() {
        let finding = |severity| Finding {
            rule: "BR-CO-15".into(),
            severity,
            message: "m".into(),
            location: None,
        };
        assert!(Report::default().is_valid());
        let warn = Report {
            findings: vec![finding(Severity::Warning)],
        };
        assert!(warn.is_valid());
        let fatal = Report {
            findings: vec![finding(Severity::Warning), finding(Severity::Fatal)],
        };
        assert!(!fatal.is_valid());
    }

    #[test]
    fn errors_name_the_business_term() {
        assert_eq!(
            ParseError::Missing { term: "BT-1" }.to_string(),
            "BT-1 is missing"
        );
        assert_eq!(
            ParseError::WrongSyntax {
                expected: Syntax::UblInvoice,
                found: Syntax::Cii
            }
            .to_string(),
            "expected a UBL invoice, found a CII invoice"
        );
    }
}
