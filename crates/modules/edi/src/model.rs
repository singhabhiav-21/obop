//! The EN 16931 semantic model: what an invoice says, whichever syntax carried it.
//!
//! Each field names its business term (BT) or group (BG) of EN 16931-1. Only the terms the readers of
//! the next slices fill are here; more are added with the slice that reads them.
//!
//! Numbers follow D9 (docs/DECISIONS.md), with no floats anywhere:
//! - an amount is [`Money`], integer minor units of its currency;
//! - a quantity, a unit price and a VAT rate are [`Decimal`] (`obop-values`, D72), because BT-129 and
//!   BT-146 may have more fractional digits than minor units hold;
//! - a value that does not fit is refused by the reader, never rounded.
//!
//! A date is [`Date`], jiff's civil date: a validated calendar day with no time zone, which is what
//! BT-2 and BT-9 are. Readers build it with [`date_from_iso`] or [`date_from_102`], never with
//! `str::parse`: jiff's parser also takes a date with a time (`2026-11-20T10:00:00`) and drops the
//! time, which an e-invoice reader must refuse.

pub use jiff::civil::Date;
pub use obop_values::Decimal;

/// A date written exactly `YYYY-MM-DD`, as UBL writes BT-2 and BT-9. Anything else, an impossible
/// day included, is `None`.
pub fn date_from_iso(s: &str) -> Option<Date> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    date_of(&s[0..4], &s[5..7], &s[8..10])
}

/// A date written exactly `YYYYMMDD`, the UNTDID 2379 format `102` in which CII writes BT-2 and BT-9.
/// Anything else, an impossible day included, is `None`.
pub fn date_from_102(s: &str) -> Option<Date> {
    if s.len() != 8 {
        return None;
    }
    date_of(s.get(0..4)?, s.get(4..6)?, s.get(6..8)?)
}

fn date_of(year: &str, month: &str, day: &str) -> Option<Date> {
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit());
    if !(digits(year) && digits(month) && digits(day)) {
        return None;
    }
    Date::new(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?).ok()
}

/// An ISO 4217 alphabetic currency code such as `EUR` or `SEK` (BT-5, BT-6): three ASCII capital
/// letters. Whether the code is assigned is checked by validation, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency([u8; 3]);

impl Currency {
    pub fn new(code: &str) -> Option<Self> {
        let b = code.as_bytes();
        (b.len() == 3 && b.iter().all(u8::is_ascii_uppercase)).then(|| Self([b[0], b[1], b[2]]))
    }

    pub fn as_str(&self) -> &str {
        // Only ASCII capitals get in through `new`.
        std::str::from_utf8(&self.0).unwrap_or("")
    }
}

/// An ISO 3166-1 alpha-2 country code such as `DE` or `SE` (BT-40, BT-55): two ASCII capital
/// letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CountryCode([u8; 2]);

impl CountryCode {
    pub fn new(code: &str) -> Option<Self> {
        let b = code.as_bytes();
        (b.len() == 2 && b.iter().all(u8::is_ascii_uppercase)).then(|| Self([b[0], b[1]]))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }
}

/// An amount in integer minor units of its currency (D9): 1190.00 EUR is `minor = 119000`. The
/// exponent is the currency's ISO 4217 minor unit; an amount with more fractional digits than that
/// is refused when it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    pub minor: i64,
    pub currency: Currency,
}

/// An identifier with its optional scheme, such as an endpoint `9900000010` with scheme `0007`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    pub value: String,
    pub scheme: Option<String>,
}

/// A VAT category code of EN 16931 (UNTDID 5305, the subset the standard allows; BT-95, BT-102,
/// BT-118, BT-151).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VatCategory {
    /// `S`, standard rate.
    Standard,
    /// `Z`, zero rated goods.
    ZeroRated,
    /// `E`, exempt from VAT.
    Exempt,
    /// `AE`, VAT reverse charge.
    ReverseCharge,
    /// `K`, intra-community supply (VAT exempt for EEA).
    IntraCommunity,
    /// `G`, free export item, VAT not charged.
    Export,
    /// `O`, services outside the scope of tax.
    OutsideScope,
    /// `L`, Canary Islands general indirect tax (IGIC).
    CanaryIslands,
    /// `M`, tax for production, services and importation in Ceuta and Melilla (IPSI).
    CeutaMelilla,
}

impl VatCategory {
    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code {
            "S" => Self::Standard,
            "Z" => Self::ZeroRated,
            "E" => Self::Exempt,
            "AE" => Self::ReverseCharge,
            "K" => Self::IntraCommunity,
            "G" => Self::Export,
            "O" => Self::OutsideScope,
            "L" => Self::CanaryIslands,
            "M" => Self::CeutaMelilla,
            _ => return None,
        })
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Standard => "S",
            Self::ZeroRated => "Z",
            Self::Exempt => "E",
            Self::ReverseCharge => "AE",
            Self::IntraCommunity => "K",
            Self::Export => "G",
            Self::OutsideScope => "O",
            Self::CanaryIslands => "L",
            Self::CeutaMelilla => "M",
        }
    }
}

/// A postal address (BG-5 seller, BG-8 buyer). EN 16931 requires the country (BR-09, BR-11); the
/// rest is optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// BT-35, BT-36, BT-162 (seller); BT-50, BT-51, BT-163 (buyer), in order.
    pub lines: Vec<String>,
    /// BT-37, BT-52.
    pub city: Option<String>,
    /// BT-38, BT-53.
    pub postal_code: Option<String>,
    /// BT-39, BT-54.
    pub subdivision: Option<String>,
    /// BT-40, BT-55.
    pub country: CountryCode,
}

/// The seller (BG-4) or the buyer (BG-7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    /// BT-27, BT-44.
    pub name: String,
    /// BT-28, BT-45.
    pub trading_name: Option<String>,
    /// BT-29, BT-46.
    pub identifiers: Vec<Identifier>,
    /// BT-30, BT-47: the legal registration identifier, such as a Swedish organisation number.
    pub legal_registration: Option<Identifier>,
    /// BT-31, BT-48: the VAT identifier, with its country prefix.
    pub vat_id: Option<String>,
    /// BT-32 (seller only): the tax registration identifier, such as "Godkänd för F-skatt".
    pub tax_registration: Option<String>,
    /// BT-34, BT-49: the electronic address.
    pub endpoint: Option<Identifier>,
    /// BG-5, BG-8.
    pub address: Address,
}

/// A preceding invoice reference (BG-3), as on a credit note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecedingInvoice {
    /// BT-25.
    pub number: String,
    /// BT-26.
    pub issue_date: Option<Date>,
}

/// The VAT of one line (BT-151, BT-152).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineVat {
    pub category: VatCategory,
    /// Percent, such as `25` or `7`; absent for category `O`.
    pub rate: Option<Decimal>,
}

/// An invoice line (BG-25).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// BT-126.
    pub id: String,
    /// BT-127.
    pub note: Option<String>,
    /// BT-129.
    pub quantity: Decimal,
    /// BT-130 (UN/ECE Recommendation 20 or 21).
    pub unit_code: String,
    /// BT-131: quantity times net price, after line allowances and charges.
    pub net_amount: Money,
    /// BT-146: the net price of one item, or of BT-149 items.
    pub net_price: Decimal,
    /// BT-149.
    pub price_base_quantity: Option<Decimal>,
    /// BT-153.
    pub item_name: String,
    /// BT-151, BT-152.
    pub vat: LineVat,
}

/// One VAT breakdown, a group per category and rate (BG-23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VatBreakdown {
    /// BT-116.
    pub taxable_amount: Money,
    /// BT-117.
    pub tax_amount: Money,
    /// BT-118.
    pub category: VatCategory,
    /// BT-119.
    pub rate: Option<Decimal>,
    /// BT-120, such as "Omvänd betalningsskyldighet".
    pub exemption_reason: Option<String>,
    /// BT-121, such as `VATEX-EU-AE`.
    pub exemption_reason_code: Option<String>,
}

/// The document totals (BG-22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    /// BT-106: the sum of the line net amounts.
    pub line_total: Money,
    /// BT-107.
    pub allowances: Option<Money>,
    /// BT-108.
    pub charges: Option<Money>,
    /// BT-109.
    pub tax_exclusive: Money,
    /// BT-110, in the invoice currency.
    pub vat_total: Option<Money>,
    /// BT-111, in the VAT accounting currency (BT-6) when it differs.
    pub vat_total_accounting: Option<Money>,
    /// BT-112.
    pub tax_inclusive: Money,
    /// BT-113.
    pub prepaid: Option<Money>,
    /// BT-114.
    pub rounding: Option<Money>,
    /// BT-115.
    pub payable: Money,
}

/// The document header of an invoice or credit note: what a reader takes from the top of the
/// document before the parties, lines and totals. The terms mean the same as on [`Invoice`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// BT-1.
    pub number: String,
    /// BT-2.
    pub issue_date: Date,
    /// BT-3 (UNTDID 1001).
    pub type_code: String,
    /// BT-5.
    pub currency: Currency,
    /// BT-9.
    pub due_date: Option<Date>,
    /// BT-10.
    pub buyer_reference: Option<String>,
    /// BT-24.
    pub specification_id: String,
    /// BG-3.
    pub preceding: Vec<PrecedingInvoice>,
}

/// An invoice or credit note in EN 16931 terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invoice {
    /// BT-1.
    pub number: String,
    /// BT-2.
    pub issue_date: Date,
    /// BT-3 (UNTDID 1001): `380` invoice, `381` credit note, and others.
    pub type_code: String,
    /// BT-5.
    pub currency: Currency,
    /// BT-6.
    pub vat_accounting_currency: Option<Currency>,
    /// BT-9.
    pub due_date: Option<Date>,
    /// BT-10: in Germany the Leitweg-ID of a public buyer.
    pub buyer_reference: Option<String>,
    /// BT-20.
    pub payment_terms: Option<String>,
    /// BT-23.
    pub business_process: Option<String>,
    /// BT-24: the specification the invoice claims to follow, such as XRechnung 3.0 or Peppol BIS 3.0.
    pub specification_id: String,
    /// BG-3.
    pub preceding: Vec<PrecedingInvoice>,
    /// BG-4.
    pub seller: Party,
    /// BG-7.
    pub buyer: Party,
    /// BG-25.
    pub lines: Vec<Line>,
    /// BG-23.
    pub vat_breakdown: Vec<VatBreakdown>,
    /// BG-22.
    pub totals: Totals,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_take_only_the_iso_shape() {
        assert_eq!(Currency::new("SEK").unwrap().as_str(), "SEK");
        for bad in ["", "EU", "EURO", "eur", "E1R", "ÉUR"] {
            assert!(Currency::new(bad).is_none(), "{bad}");
        }
        assert_eq!(CountryCode::new("SE").unwrap().as_str(), "SE");
        for bad in ["", "S", "SWE", "se", "S1"] {
            assert!(CountryCode::new(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn every_vat_category_code_reads_back() {
        for code in ["S", "Z", "E", "AE", "K", "G", "O", "L", "M"] {
            assert_eq!(VatCategory::from_code(code).unwrap().code(), code);
        }
        assert!(VatCategory::from_code("B").is_none());
        assert!(VatCategory::from_code("s").is_none());
    }

    #[test]
    fn a_date_is_a_validated_calendar_day_in_exactly_one_shape() {
        let d = date_from_iso("2026-11-20").unwrap();
        assert_eq!(d.to_string(), "2026-11-20");
        assert_eq!(date_from_102("20261120"), Some(d));
        assert_eq!(date_from_iso("2028-02-29"), Date::new(2028, 2, 29).ok());
        for bad in [
            "2026-02-30",
            "2027-02-29",
            "2026-13-01",
            "2026-11-20T10:00:00",
            "2026-11-20Z",
            " 2026-11-20",
            "2026-1-20",
            "2026/11/20",
            "+026-11-20",
            "2026-11-2a",
            "",
        ] {
            assert_eq!(date_from_iso(bad), None, "{bad}");
        }
        for bad in [
            "20260230",
            "2026112",
            "202611200",
            "2026-1120",
            "+0261120",
            "２０２６1120",
        ] {
            assert_eq!(date_from_102(bad), None, "{bad}");
        }
        // Why the readers do not use `str::parse`: jiff takes a date with a time and drops the time.
        assert_eq!("2026-11-20T10:00:00".parse::<Date>().ok(), Some(d));
    }

    /// D9: no floating point in the model. A text check of this file, so a field typed `f64` fails
    /// here before anyone writes an amount into it.
    #[test]
    fn the_model_has_no_floats() {
        let src = include_str!("model.rs");
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for float in ["f32", "f64"] {
            let hit = code.match_indices(float).any(|(i, _)| {
                let before = code[..i].chars().last().unwrap_or(' ');
                let after = code[i + 3..].chars().next().unwrap_or(' ');
                !before.is_alphanumeric()
                    && before != '_'
                    && !after.is_alphanumeric()
                    && after != '_'
                    && after != '"'
            });
            assert!(!hit, "model.rs uses {float}");
        }
    }
}
