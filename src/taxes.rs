use chrono::Days;

use crate::data::{Bestand, Security, SecurityType, Steuern, Transaction, TransactionKind};
use crate::scraper::{ReportRow, Scraper};

pub async fn do_taxes(scraper: &mut Scraper, security: &mut Security) {
    // transaktionen sortieren
    security.transaktionen.sort_by_key(|t| t.datum);

    let mut bestand = Bestand::default();

    let meldungen = if security.typ == SecurityType::Etf {
        let report = scraper.fetch_reports(&security.isin).await.unwrap();
        security.name = report.name;

        report.rows
    } else {
        vec![]
    };
    let mut meldungen = meldungen.into_iter().peekable();

    // von hinten nach vorne:

    let transaktionen = std::mem::take(&mut security.transaktionen);
    let mut transaktionen = transaktionen.into_iter().peekable();

    let mut meldung: Option<ReportRow> = None;

    loop {
        let next_transaktion = transaktionen.peek();
        loop {
            let Some(next_meldung) = meldungen.peek_mut() else {
                break;
            };
            let transaktion_ist_vorher = next_transaktion
                .map(|t| t.datum < next_meldung.date)
                .unwrap_or(false);
            if transaktion_ist_vorher {
                break;
            }
            let gibt_bestand = bestand.stück > 0.0;
            if gibt_bestand {
                // meldung anwenden
                scraper.fetch_report_details(next_meldung).await.unwrap();

                if next_meldung.is_yearly_report {
                    let mut steuern = Steuern::default();
                    steuern_für_meldung(&mut bestand, &mut steuern, next_meldung);

                    security.transaktionen.push(Transaction {
                        datum: next_meldung.date,
                        typ: TransactionKind::Jahresmeldung {
                            melde_id: next_meldung.report_id,
                        },
                        bestand: bestand.clone(),
                        steuern,
                    });
                } else {
                    // die nächste transaktion sollte eine ausschüttung sein
                    let nächste_transaktion_ist_ausschüttung = next_transaktion
                        .map(|t| {
                            matches!(t.typ, TransactionKind::Ausschüttung { .. })
                                && t.datum < next_meldung.date + Days::new(5)
                        })
                        .unwrap_or(false);
                    if nächste_transaktion_ist_ausschüttung {
                        // meldung speichern für transaktion
                        meldung = meldungen.next();
                        break;
                    } else {
                        panic!("meldung ohne Ausschüttung");
                    }
                }
            } // else: überspringen
            meldungen.next();
        }

        let Some(mut transaktion) = transaktionen.next() else {
            break;
        };

        let steuern = &mut transaktion.steuern;
        match &mut transaktion.typ {
            // Einkommensteuergesetz 1988, Fassung vom 02.09.2023:
            // <https://www.ris.bka.gv.at/GeltendeFassung.wxe?Abfrage=Bundesnormen&Gesetzesnummer=10004570>
            TransactionKind::Kauf { stück, preis } => {
                // Laut § 27a (4) 3. gilt:
                // [B]ei Erwerb in zeitlicher Aufeinanderfolge [ist] der gleitende
                // Durchschnittspreis […] anzusetzen.

                let stück_neu = bestand.stück + *stück;
                let preis_neu = (bestand.summe() + (*stück * *preis)) / stück_neu;

                bestand.preis = preis_neu;
                bestand.stück = stück_neu;
            }
            TransactionKind::Verkauf { stück, preis } => {
                // Laut § 27a (3) 2. gilt:
                // Als Einkünfte anzusetzen sind
                // [b]ei realisierten Wertsteigerungen […] der Unterschiedsbetrag
                // zwischen dem Veräußerungserlös […] und den Anschaffungskosten.

                let einstand = *stück * bestand.preis;
                let erlös = *stück * *preis;

                if erlös > einstand {
                    steuern.wertsteigerungen_994 = erlös - einstand;
                } else {
                    steuern.wertverluste_892 = einstand - erlös;
                }

                // TODO: floating point math is hard -_-
                if bestand.stück == 0.0 {
                    bestand.preis = 0.0;
                }
            }
            TransactionKind::Split { faktor } => {
                bestand.stück *= *faktor;
                bestand.preis /= *faktor;
            }
            TransactionKind::Dividende { brutto, auszahlung } => {
                // Laut § 27a (3) 1. gilt:
                // [D]ie bezogenen Kapitalerträge.

                assert_eq!(security.typ, SecurityType::Aktie);

                steuern.dividendenerträge_863 = *brutto;

                let gezahlte_quellensteuer = *brutto - *auszahlung;
                if security.isin.starts_with("AT") {
                    // sind die quellensteuern für AT aktien im ausland jetzt 899 oder 998?
                    steuern.gezahlte_kest_899 = gezahlte_quellensteuer;
                } else {
                    // TODO: hängt vom Land ab
                    let anrechenbarer_quellensteuersatz = 0.15;
                    steuern.anrechenbare_quellensteuer_998 =
                        (*brutto * anrechenbarer_quellensteuersatz).min(gezahlte_quellensteuer);
                }
            }
            TransactionKind::Ausschüttung { brutto, melde_id } => {
                assert_eq!(security.typ, SecurityType::Etf);

                if let Some(meldung) = meldung.take() {
                    // ausschüttung mit meldung
                    *melde_id = meldung.report_id;
                    steuern_für_meldung(&mut bestand, steuern, &meldung);
                } else {
                    // ausschüttung ohne meldung
                    steuern.ausschüttungen_898 = *brutto;
                }
            }
            TransactionKind::Jahresmeldung { .. } => unreachable!("covered above"),
        }
        transaktion.bestand = bestand.clone();

        security.transaktionen.push(transaktion);
    }
}

fn steuern_für_meldung(bestand: &mut Bestand, steuern: &mut Steuern, meldung: &ReportRow) {
    steuern.ausschüttungen_898 = meldung.StB_E1KV_Ausschuettungen / meldung.rate * bestand.stück;
    steuern.ausschüttungsgleiche_erträge_937 =
        meldung.StB_E1KV_AGErtraege / meldung.rate * bestand.stück;
    steuern.anrechenbare_quellensteuer_998 =
        meldung.StB_E1KV_anzurechnende_ausl_Quellensteuer / meldung.rate * bestand.stück;

    bestand.preis += meldung.StB_E1KV_Korrekturbetrag_saldiert / meldung.rate;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_do_taxes() {
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
        let mut scraper = Scraper::new();
        do_taxes(&mut scraper, &mut security).await;
        dbg!(security);
    }
}
