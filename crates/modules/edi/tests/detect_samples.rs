//! Syntax detection over every e-invoice sample of testdata/e-invoices (its README and
//! provenance.json): obop's own files, the committed KoSIT business cases, and the fetched CEN and
//! OpenPeppol examples when they are there.
//!
//! The fetched files come from `deploy/fetch-einvoice-samples.sh` (into `target/e-invoice-samples` or
//! `OBOP_EINVOICE_SAMPLES_DIR`). Without them that test says so and passes; with
//! `OBOP_REQUIRE_EINVOICE_SAMPLES=1`, as CI sets it, a missing file fails it.

mod common;

use common::{read, samples};
use edi::{DetectError, Syntax, detect};
use serde_json::Value;

#[test]
fn own_samples_are_detected_as_expected_json_says() {
    let own = samples().join("own");
    let expected: Value =
        serde_json::from_str(&String::from_utf8(read(&own.join("expected.json"))).unwrap())
            .unwrap();
    let entries = expected.as_array().unwrap();
    assert_eq!(entries.len(), 13);
    for e in entries {
        let file = e["file"].as_str().unwrap();
        let got = detect(&read(&own.join(file)));
        let format = e["fields"]["format"].as_str();
        match (file, format) {
            ("08-not-an-einvoice.xml", _) => assert_eq!(
                got,
                Err(DetectError::NotAnInvoice {
                    namespace: None,
                    name: "order".into()
                }),
                "{file}"
            ),
            ("09-broken-xml.xml", _) => {
                assert!(
                    matches!(got, Err(DetectError::NotXml(_))),
                    "{file}: {got:?}"
                )
            }
            (_, Some("xrechnung-cii")) => assert_eq!(got, Ok(Syntax::Cii), "{file}"),
            (_, Some("xrechnung-ubl" | "peppol-bis-3")) => {
                assert_eq!(got, Ok(Syntax::UblInvoice), "{file}")
            }
            _ => panic!("{file}: no expectation for format {format:?}"),
        }
    }
}

#[test]
fn every_kosit_case_is_detected_by_its_syntax() {
    let kosit = samples().join("kosit");
    let mut seen = 0;
    for group in ["standard", "extension"] {
        for entry in std::fs::read_dir(kosit.join(group)).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let got = detect(&read(&path));
            if name.ends_with("_uncefact.xml") {
                assert_eq!(got, Ok(Syntax::Cii), "{name}");
            } else if name.ends_with("_ubl.xml") {
                assert!(
                    matches!(got, Ok(Syntax::UblInvoice | Syntax::UblCreditNote)),
                    "{name}: {got:?}"
                );
            } else {
                panic!("{name}: neither _ubl nor _uncefact");
            }
            seen += 1;
        }
    }
    assert_eq!(seen, 48);
}

#[test]
fn fetched_cen_and_peppol_examples_are_detected_when_present() {
    let Some(fetched) = common::fetched() else {
        return;
    };
    for f in &fetched {
        let got = detect(&read(&f.path));
        if f.src.starts_with("cii/") {
            assert_eq!(got, Ok(Syntax::Cii), "{}", f.name);
        } else {
            // CEN ubl/examples and every OpenPeppol example or snippet are UBL.
            assert!(
                matches!(got, Ok(Syntax::UblInvoice | Syntax::UblCreditNote)),
                "{}: {got:?}",
                f.name
            );
        }
    }
    assert_eq!(fetched.len(), 58);
}
