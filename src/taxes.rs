use crate::data::{Bestand, Security, SecurityType, Transaction, TransactionKind};

pub fn do_taxes(security: &mut Security) {
    // transaktionen sortieren
    security.transaktionen.sort_by_key(|t| t.datum);

    let mut bestand = Bestand::default();

    // von hinten nach vorne:
    for transaktion in &mut security.transaktionen {
        let steuern = &mut transaktion.steuern;
        match transaktion.typ {
            // Einkommensteuergesetz 1988, Fassung vom 02.09.2023:
            // <https://www.ris.bka.gv.at/GeltendeFassung.wxe?Abfrage=Bundesnormen&Gesetzesnummer=10004570>
            TransactionKind::Kauf { stück, preis } => {
                // Laut § 27a (4) 3. gilt:
                // [B]ei Erwerb in zeitlicher Aufeinanderfolge [ist] der gleitende
                // Durchschnittspreis […] anzusetzen.

                let stück_neu = bestand.stück + stück;
                let preis_neu = (bestand.summe() + (stück * preis)) / stück_neu;

                bestand.preis = preis_neu;
                bestand.stück = stück_neu;
            }
            TransactionKind::Verkauf { stück, preis } => {
                // Laut § 27a (3) 2. gilt:
                // Als Einkünfte anzusetzen sind
                // [b]ei realisierten Wertsteigerungen […] der Unterschiedsbetrag
                // zwischen dem Veräußerungserlös […] und den Anschaffungskosten.

                let erlös = stück * preis;
                let gewinn = stück * (preis - bestand.preis);

                bestand.stück -= stück;
                // TODO: floating point math is hard -_-
                if bestand.stück == 0.0 {
                    bestand.preis = 0.0;
                }
                steuern.erlös = erlös;
                // TODO: gewinn oder verlust?
                steuern.gewinn = gewinn;
            }
            TransactionKind::Split { faktor } => {
                bestand.stück *= faktor;
                bestand.preis /= faktor;
            }
            TransactionKind::Dividende { brutto, ertrag } => {
                // Laut § 27a (3) 1. gilt:
                // [D]ie bezogenen Kapitalerträge.

                assert_eq!(security.typ, SecurityType::Aktie);

                // TODO: `AT…` Aktien
                steuern.erlös = brutto;
                let gezahlte_quellensteuer = brutto - ertrag;
                // TODO: hängt vom Land ab
                let anrechenbarer_quellensteuersatz = 0.15;
                steuern.anrechenbare_quellensteuer =
                    (brutto * anrechenbarer_quellensteuersatz).min(gezahlte_quellensteuer);
            }
            TransactionKind::Ausschüttung { brutto } => {
                assert_eq!(security.typ, SecurityType::Etf);

                // TODO: korrelieren mit meldungen…
                todo!()
            }
        }
        transaktion.bestand = bestand.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_do_taxes() {
        let mut security: Security = serde_yaml::from_str(
            r#"
typ: aktie
name: Foo
isin: DE000
transaktionen:
- kauf: [2023-01-01, 40, 30.23]
- verkauf: [2023-02-02, 40, 32]
        "#,
        )
        .unwrap();
        do_taxes(&mut security);
        dbg!(security);
    }
}
