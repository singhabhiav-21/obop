//! `edi`: EN 16931 e-invoices, the one internal invoice model behind every syntax (docs/LOCALIZATION.md
//! 2.3; docs/ARCHITECTURE.md, the E-invoicing row and the module graph `invoicing & tax --> edi`).
//!
//! It has the model ([`model`]), the [`DocFormat`] trait every syntax implements, the detection of
//! the syntax from the root element ([`detect`]) and the start of the UBL reader ([`ubl`],
//! XRechnung-UBL and Peppol BIS Billing 3.0), which reads the document header ([`Header`]) so far.
//! CII comes after UBL.
//!
//! `edi` ships with core (LOCALIZATION 1.2) and never depends on a country pack; packs depend on it.
//! The `edi-api` split of ARCHITECTURE's crate layout is not made yet: the types and the trait live
//! in their own files so that it can be.

pub mod detect;
pub mod format;
pub mod model;
pub mod ubl;

pub use detect::{DetectError, MAX_INPUT_BYTES, MAX_NODES, Syntax, detect};
pub use format::{BuildError, DocFormat, Finding, ParseError, Report, Severity};
pub use model::{
    Address, CountryCode, Currency, Date, Decimal, Header, Identifier, Invoice, Line, LineVat,
    Money, Party, PrecedingInvoice, Totals, VatBreakdown, VatCategory, date_from_102,
    date_from_iso,
};
pub use ubl::UblFormat;
