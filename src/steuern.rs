use anyhow::Result;
use chrono::{Datelike, Days};
use num_traits::identities::Zero;

use crate::scraper::{ReportRow, Scraper};
use crate::{format, SteuerAusschüttung, SteuerDividende, SteuerVerkauf, WertpapierTyp};
use crate::{Bestand, Datum, Jahr, Steuer, Transaktion, TransaktionsTyp, Wertpapier, Zahl};

#[derive(Debug)]
struct Rechner {
    heute: Datum,
    scraper: Scraper,
}
impl Rechner {
    pub fn new() -> Self {
        let heute = chrono::Local::now().date_naive();
        let scraper = Scraper::new();
        Self { heute, scraper }
    }

    pub async fn wertpapier_auswerten(
        &mut self,
        wertpapier: format::Wertpapier,
    ) -> Result<Wertpapier> {
        let format::Wertpapier {
            typ,
            mut name,
            isin,
            mut transaktionen,
        } = wertpapier;

        transaktionen.sort_by_key(|t| t.datum());
        let mut transaktionen = transaktionen.into_iter().peekable();

        let meldungen = if typ == WertpapierTyp::Etf {
            let meldungen = self.scraper.fetch_reports(&isin).await?;
            name = meldungen.name;

            meldungen.rows
        } else {
            vec![]
        };
        let mut meldungen = meldungen.into_iter().peekable();

        let mut jahre = vec![];
        let mut bestand = Bestand::default();
        let mut meldung = None;

        loop {
            // ======== TODO ========
            let nächste_transaktion = transaktionen.peek();
            loop {
                const MELDUNG_DATUM_ABWEICHUNG: Days = Days::new(7);

                let Some(nächste_meldung) = meldungen.peek_mut() else {
                    break;
                };
                let transaktion_ist_vorher = nächste_transaktion
                    .map(|t| {
                        let datum = t.datum();
                        if !nächste_meldung.is_yearly_report
                            && matches!(t, format::Transaktion::Ausschüttung { .. })
                        {
                            datum + MELDUNG_DATUM_ABWEICHUNG < nächste_meldung.date
                        } else {
                            datum < nächste_meldung.date
                        }
                    })
                    .unwrap_or(false);
                if transaktion_ist_vorher {
                    break;
                }
                let gibt_bestand = !bestand.stück.is_zero();
                if gibt_bestand {
                    // meldung anwenden
                    self.scraper.fetch_report_details(nächste_meldung).await?;

                    if nächste_meldung.is_yearly_report {
                        let steuer = meldung_berechnen(&mut bestand, nächste_meldung);
                        let transaktion = Transaktion {
                            datum: nächste_meldung.date,
                            bestand,
                            typ: TransaktionsTyp::Jahresmeldung {
                                melde_id: nächste_meldung.report_id,
                            },
                            steuer,
                        };
                        transaktion_anfügen(&mut jahre, transaktion);
                    } else {
                        // die nächste transaktion sollte eine ausschüttung sein
                        let nächste_transaktion_ist_ausschüttung = nächste_transaktion
                            .map(|t| {
                                matches!(t, format::Transaktion::Ausschüttung { .. })
                                    && t.datum() < nächste_meldung.date + MELDUNG_DATUM_ABWEICHUNG
                            })
                            .unwrap_or(false);
                        if nächste_transaktion_ist_ausschüttung {
                            // meldung speichern für transaktion
                            meldung = meldungen.next();
                            break;
                        } else {
                            dbg!(name, nächste_transaktion, nächste_meldung);
                            panic!("meldung ohne Ausschüttung");
                        }
                    }
                } // else: überspringen
                meldungen.next();
            }
            // ======== TODO ========

            let Some(transaktion) = transaktionen.next() else {
                break;
            };

            let mut datum = transaktion.datum();

            let (bestand_, typ, steuer) = match transaktion {
                format::Transaktion::Kauf(_, format::Zahl(stück), format::Zahl(preis)) => {
                    kauf_berechnen(bestand, stück, preis)
                }
                format::Transaktion::Verkauf(_, format::Zahl(stück), format::Zahl(preis)) => {
                    verkauf_berechnen(bestand, stück, preis)
                }
                format::Transaktion::Split(_, format::Zahl(faktor)) => {
                    split_berechnen(bestand, faktor)
                }
                format::Transaktion::Dividende(
                    _,
                    format::Zahl(brutto),
                    format::Zahl(auszahlung),
                ) => {
                    anyhow::ensure!(typ == WertpapierTyp::Aktie, "nur Aktien haben Dividenden");

                    dividende_berechnen(bestand, &isin, brutto, auszahlung)
                }
                format::Transaktion::Ausschüttung(_, format::Zahl(auszahlung)) => {
                    anyhow::ensure!(typ == WertpapierTyp::Etf, "nur ETFs haben Ausschüttungen");

                    ausschüttung_berechnen(bestand, auszahlung, meldung.take())
                }
            };
            bestand = bestand_;
            let transaktion = Transaktion {
                datum,
                bestand,
                typ,
                steuer,
            };
            transaktion_anfügen(&mut jahre, transaktion);
        }

        jahre_abschließen(&mut jahre, self.heute.year());

        Ok(Wertpapier {
            typ,
            name,
            isin,
            jahre,
        })
    }
}

type Ergebnis = (Bestand, TransaktionsTyp, Steuer);

// Laut § 27a (4) 3. gilt:
// [B]ei Erwerb in zeitlicher Aufeinanderfolge [ist] der gleitende
// Durchschnittspreis […] anzusetzen.
fn kauf_berechnen(bestand: Bestand, stück: Zahl, preis: Zahl) -> Ergebnis {
    let stück_neu = bestand.stück + stück;
    let preis_neu = (bestand.summe() + (stück * preis)) / stück_neu;

    (
        Bestand {
            stück: stück_neu,
            preis: preis_neu,
        },
        TransaktionsTyp::Kauf { stück, preis },
        Steuer::Keine,
    )
}

// Laut § 27a (3) 2. gilt:
// Als Einkünfte anzusetzen sind
// [b]ei realisierten Wertsteigerungen […] der Unterschiedsbetrag
// zwischen dem Veräußerungserlös […] und den Anschaffungskosten.
fn verkauf_berechnen(mut bestand: Bestand, stück: Zahl, preis: Zahl) -> Ergebnis {
    let einstand = stück * bestand.preis;
    let erlös = stück * preis;

    bestand.stück -= stück;
    if bestand.stück.is_zero() {
        bestand.preis = 0.into();
    }

    let mut steuer = SteuerVerkauf::default();
    if erlös > einstand {
        steuer.überschüsse_994 = erlös - einstand;
    } else {
        steuer.verluste_892 = einstand - erlös;
    }

    (
        bestand,
        TransaktionsTyp::Verkauf { stück, preis },
        Steuer::Verkauf(steuer),
    )
}

fn split_berechnen(mut bestand: Bestand, faktor: Zahl) -> Ergebnis {
    bestand.stück *= faktor;
    bestand.preis /= faktor;

    (bestand, TransaktionsTyp::Split { faktor }, Steuer::Keine)
}

fn dividende_berechnen(bestand: Bestand, isin: &str, brutto: Zahl, auszahlung: Zahl) -> Ergebnis {
    let mut steuer = SteuerDividende::default();

    let gezahlte_quellensteuer = brutto - auszahlung;
    if isin.starts_with("AT") {
        // sind die quellensteuern für AT aktien im ausland jetzt 899 oder 998?
        steuer.gezahlte_inländische_kest_899 = gezahlte_quellensteuer;
    } else {
        // TODO: hängt vom Land ab?
        let anrechenbarer_quellensteuersatz = Zahl::new(15, 100);
        steuer.anrechenbare_quellensteuer_998 =
            (brutto * anrechenbarer_quellensteuersatz).min(gezahlte_quellensteuer);
    }

    (
        bestand,
        TransaktionsTyp::Dividende { brutto, auszahlung },
        Steuer::Dividende(steuer),
    )
}

fn ausschüttung_berechnen(
    mut bestand: Bestand,
    auszahlung: Zahl,
    meldung: Option<ReportRow>,
) -> Ergebnis {
    let mut steuer = SteuerAusschüttung::default();

    let Some(meldung) = meldung else {
        steuer.ausschüttungen_898 = auszahlung;
        return (
            bestand,
            TransaktionsTyp::Ausschüttung {
                brutto: auszahlung,
                melde_id: None,
            },
            Steuer::Ausschüttung(steuer),
        );
    };

    let melde_id = Some(meldung.report_id);
    let steuer = meldung_berechnen(&mut bestand, &meldung);
    (
        bestand,
        TransaktionsTyp::Ausschüttung {
            brutto: auszahlung,
            melde_id,
        },
        steuer,
    )
}

fn meldung_berechnen(bestand: &mut Bestand, meldung: &ReportRow) -> Steuer {
    let prostück = bestand.stück / meldung.rate;

    let mut steuer = SteuerAusschüttung::default();
    steuer.ausschüttungen_898 = meldung.StB_E1KV_Ausschuettungen * prostück;
    steuer.ausschüttungsgleiche_erträge_937 = meldung.StB_E1KV_AGErtraege * prostück;
    steuer.anrechenbare_quellensteuer_998 =
        meldung.StB_E1KV_anzurechnende_ausl_Quellensteuer * prostück;

    bestand.preis += meldung.StB_E1KV_Korrekturbetrag_saldiert / meldung.rate;

    Steuer::Ausschüttung(steuer)
}

fn transaktion_anfügen(jahre: &mut Vec<Jahr>, transaktion: Transaktion) {
    let jahr = transaktion.datum.year();
    let jahr = if let Some(letztes_jahr) = jahre.last_mut() {
        if letztes_jahr.jahr < jahr {
            let bestand = letztes_jahr.bestand_ende;
            jahre.push(Jahr {
                jahr,
                bestand_anfang: bestand,
                ..Default::default()
            });
            jahre.last_mut().unwrap()
        } else {
            letztes_jahr
        }
    } else {
        jahre.push(Jahr {
            jahr,
            ..Default::default()
        });
        jahre.last_mut().unwrap()
    };
    jahr.bestand_ende = transaktion.bestand;
    jahr.transaktionen.push(transaktion);
}

fn jahre_abschließen(jahre: &mut Vec<Jahr>, jahr: i32) {
    if let Some(letztes_jahr) = jahre.last() {
        if letztes_jahr.bestand_ende.stück.is_zero() || letztes_jahr.jahr >= jahr {
            return; // nichts zu tun
        }
        let bestand = letztes_jahr.bestand_ende;
        for jahr in letztes_jahr.jahr..=jahr {
            jahre.push(Jahr {
                jahr,
                bestand_anfang: bestand,
                bestand_ende: bestand,
                transaktionen: vec![],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_do_taxes() {
        let wertpapier = serde_yaml::from_str(
            r#"
typ: aktie
name: Foo
isin: DE000
transaktionen:
- kauf: [2021-01-01, 40, 30.23]
- verkauf: [2022-02-02, 40, 32]
        "#,
        )
        .unwrap();
        let mut rechner = Rechner::new();
        let wertpapier = rechner.wertpapier_auswerten(wertpapier).await.unwrap();
        dbg!(wertpapier);
    }
}
