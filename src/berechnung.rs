use anyhow::{Context, Result};
use chrono::{Datelike, Days};
use num_traits::identities::Zero;

use crate::format;
use crate::kursdaten::Kursabfrage;
use crate::meldungen::Scraper;
use crate::steuern::{
    ausgliederung_berechnen, ausschüttung_berechnen, dividende_berechnen, einbuchung_berechnen,
    kauf_berechnen, meldung_berechnen, spitzenverwertung_berechnen, split_berechnen,
    verkauf_berechnen,
};
use crate::{Bestand, Datum, Jahr, Transaktion, TransaktionsTyp, Wertpapier, WertpapierTyp};

#[derive(Debug)]
pub struct Rechner {
    pub heute: Datum,
    scraper: Scraper,
    kursdaten: Kursabfrage,
}
impl Rechner {
    pub fn new() -> Self {
        let heute = chrono::Local::now().date_naive();
        let scraper = Scraper::new();
        let kursdaten = Kursabfrage::new();
        Self {
            heute,
            scraper,
            kursdaten,
        }
    }

    pub async fn wertpapier_auswerten(
        &mut self,
        wertpapier: format::Wertpapier,
    ) -> Result<Wertpapier> {
        let format::Wertpapier {
            typ,
            mut name,
            isin,
            mut symbol,
            mut transaktionen,
            ..
        } = wertpapier;

        transaktionen.sort_by_key(|t| t.datum());
        let mut transaktionen = transaktionen.into_iter().peekable();

        let mut meldungen = vec![];

        if typ == WertpapierTyp::Etf {
            let meldungsdaten = self.scraper.fetch_meldungen(&isin).await?;

            meldungen = meldungsdaten.meldungen;
            name = meldungsdaten.name;
        } else {
            let suche = symbol.as_deref().unwrap_or(isin.as_str());
            if let Some(metadaten) = self.kursdaten.aktie_suchen(suche).await? {
                symbol = Some(metadaten.symbol);
                name = metadaten.name;
            }
        }

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
                        if !nächste_meldung.ist_jahresmeldung
                            && matches!(t, format::Transaktion::Ausschüttung { .. })
                        {
                            datum + MELDUNG_DATUM_ABWEICHUNG < nächste_meldung.datum
                        } else {
                            datum < nächste_meldung.datum
                        }
                    })
                    .unwrap_or(false);
                if transaktion_ist_vorher {
                    break;
                }
                let gibt_bestand = !bestand.stück.is_zero();
                if gibt_bestand {
                    // meldung anwenden
                    self.scraper.fetch_meldungs_details(nächste_meldung).await?;

                    if nächste_meldung.ist_jahresmeldung {
                        let steuer = meldung_berechnen(&mut bestand, nächste_meldung);
                        let transaktion = Transaktion {
                            datum: nächste_meldung.datum,
                            bestand,
                            typ: TransaktionsTyp::Jahresmeldung {
                                melde_id: nächste_meldung.melde_id,
                            },
                            steuer,
                        };
                        transaktion_anfügen(&mut jahre, transaktion);
                    } else {
                        // die nächste transaktion sollte eine ausschüttung sein
                        let nächste_transaktion_ist_ausschüttung = nächste_transaktion
                            .map(|t| {
                                matches!(t, format::Transaktion::Ausschüttung { .. })
                                    && t.datum() < nächste_meldung.datum + MELDUNG_DATUM_ABWEICHUNG
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
                format::Transaktion::Ausgliederung(_, format::Zahl(faktor), isin) => {
                    let symbol = symbol.as_deref().context("Aktie sollte ein Symbol haben")?;
                    let eigener_kurs = self.kursdaten.kurs_abrufen(symbol, datum).await?;
                    let andere_metadaten = self
                        .kursdaten
                        .aktie_suchen(&isin)
                        .await?
                        .context("Aktie sollte gefunden werden")?;
                    let anderer_kurs = self
                        .kursdaten
                        .kurs_abrufen(&andere_metadaten.symbol, datum)
                        .await?;

                    ausgliederung_berechnen(
                        bestand,
                        faktor,
                        isin,
                        eigener_kurs.open,
                        anderer_kurs.open,
                    )
                }
                format::Transaktion::Einbuchung(_, format::Zahl(stück)) => {
                    let symbol = symbol.as_deref().context("Aktie sollte ein Symbol haben")?;
                    let kurs = self.kursdaten.kurs_abrufen(symbol, datum).await?;
                    einbuchung_berechnen(bestand, stück, kurs.open)
                }
                format::Transaktion::Spitzenverwertung(
                    _,
                    format::Zahl(stück),
                    format::Zahl(preis),
                ) => spitzenverwertung_berechnen(bestand, stück, preis),

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

                    let meldung = meldung.take();
                    if let Some(meldung) = &meldung {
                        // wir nutzen hier das datum der meldung
                        // die tatsächliche auszahlung kann wegen wochenende usw verzögert sein
                        datum = meldung.datum;
                    }
                    ausschüttung_berechnen(bestand, auszahlung, meldung)
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

impl Default for Rechner {
    fn default() -> Self {
        Self::new()
    }
}

fn transaktion_anfügen(jahre: &mut Vec<Jahr>, transaktion: Transaktion) {
    let jahr = transaktion.datum.year();
    jahre_abschließen(jahre, jahr);
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
        for jahr in letztes_jahr.jahr + 1..=jahr {
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
    async fn test_berechnung() {
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
