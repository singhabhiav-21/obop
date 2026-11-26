//! The UBL header (BT-1, 2, 3, 5, 9, 10, 24 and BG-3) over the samples of testdata/e-invoices: obop's
//! own UBL files against expected.json, every KoSIT `*_ubl.xml`, and the fetched CEN and OpenPeppol
//! UBL examples when they are there (see `common::fetched`).

mod common;

use common::{read, samples};
use edi::ubl::read_header;
use edi::{ParseError, Syntax, date_from_iso};
use serde_json::Value;

const XRECHNUNG_3: &str = "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0";
const PEPPOL_BIS_3: &str =
    "urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0";

#[test]
fn own_ubl_samples_match_the_header_of_expected_json() {
    let own = samples().join("own");
    let expected: Value =
        serde_json::from_str(&String::from_utf8(read(&own.join("expected.json"))).unwrap())
            .unwrap();
    let ubl: Vec<&Value> = expected
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            matches!(
                e["fields"]["format"].as_str(),
                Some("xrechnung-ubl" | "peppol-bis-3")
            )
        })
        .collect();
    let files: Vec<&str> = ubl.iter().map(|e| e["file"].as_str().unwrap()).collect();
    assert_eq!(
        files,
        [
            "02-valid-ubl-consulting.xml",
            "10-valid-peppol-se-25.xml",
            "11-valid-peppol-se-12.xml",
            "12-valid-peppol-se-6-and-25.xml",
            "13-valid-peppol-se-reverse-charge.xml",
        ]
    );

    for e in ubl {
        let file = e["file"].as_str().unwrap();
        let f = &e["fields"];
        let inv = read_header(&read(&own.join(file))).unwrap_or_else(|err| panic!("{file}: {err}"));
        let date = |v: &Value| v.as_str().map(|s| date_from_iso(s).unwrap());

        assert_eq!(inv.number, f["number"].as_str().unwrap(), "{file} BT-1");
        assert_eq!(Some(inv.issue_date), date(&f["issue_date"]), "{file} BT-2");
        assert_eq!(
            inv.type_code,
            f["type_code"].as_str().unwrap(),
            "{file} BT-3"
        );
        assert_eq!(
            inv.currency.as_str(),
            f["currency"].as_str().unwrap(),
            "{file} BT-5"
        );
        assert_eq!(inv.due_date, date(&f["due_date"]), "{file} BT-9");
        let preceding: Vec<&str> = inv.preceding.iter().map(|p| p.number.as_str()).collect();
        let want: Vec<&str> = f["preceding_invoice"].as_str().into_iter().collect();
        assert_eq!(preceding, want, "{file} BG-3");

        // expected.json has no BT-10 or BT-24; these are the values written in the files.
        let (spec, buyer_ref) = if f["format"] == "xrechnung-ubl" {
            (XRECHNUNG_3, "PO-0815")
        } else {
            (PEPPOL_BIS_3, "Anna Testsson")
        };
        assert_eq!(inv.specification_id, spec, "{file} BT-24");
        assert_eq!(
            inv.buyer_reference.as_deref(),
            Some(buyer_ref),
            "{file} BT-10"
        );
    }
}

#[test]
fn every_kosit_ubl_case_reads_its_header() {
    let kosit = samples().join("kosit");
    let mut seen = 0;
    for group in ["standard", "extension"] {
        for entry in std::fs::read_dir(kosit.join(group)).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let got = read_header(&read(&path));
            if name.ends_with("_ubl.xml") {
                let inv = got.unwrap_or_else(|e| panic!("{name}: {e}"));
                assert!(
                    inv.specification_id.contains("xrechnung"),
                    "{name}: BT-24 {}",
                    inv.specification_id
                );
                seen += 1;
            } else {
                // The CII half of each case is refused as the wrong syntax, not misread.
                assert_eq!(
                    got,
                    Err(ParseError::WrongSyntax {
                        expected: Syntax::UblInvoice,
                        found: Syntax::Cii
                    }),
                    "{name}"
                );
            }
        }
    }
    assert_eq!(seen, 26);
}

#[test]
fn fetched_cen_and_peppol_ubl_examples_read_their_header_when_present() {
    let Some(fetched) = common::fetched() else {
        return;
    };
    let mut seen = 0;
    for f in fetched.iter().filter(|f| !f.src.starts_with("cii/")) {
        if let Err(e) = read_header(&read(&f.path)) {
            panic!("{}: {e}", f.name);
        }
        seen += 1;
    }
    assert_eq!(seen, 43);
}
