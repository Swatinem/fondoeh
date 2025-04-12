use anyhow::{Context, Result};
use chrono::{Datelike, Days};
use num_traits::identities::Zero;

use crate::cacher::Cacher;
use crate::format;
use crate::kursdaten::Kursabfrage;
use crate::meldungen::Meldungen;
use crate::steuern::{
    ausgliederung_berechnen, ausschüttung_berechnen, ausschüttungsgleiche_beträge_berechnen,
    dividende_berechnen, einbuchung_berechnen, kauf_berechnen, meldung_berechnen,
    spitzenverwertung_berechnen, split_berechnen, verkauf_berechnen,
};
use crate::waehrungen::{Kurs, Währungen};
use crate::{Bestand, Datum, Jahr, Transaktion, TransaktionsTyp, Wertpapier, WertpapierTyp, Zahl};

#[derive(Debug)]
pub struct Rechner {
    pub heute: Datum,
    meldungen: Meldungen,
    kursabfrage: Kursabfrage,
    währungen: Währungen,
}
impl Rechner {
    pub async fn new() -> Result<Self> {
        let heute = chrono::Local::now().date_naive();
        let cacher = Cacher::new().await?;
        let währungen = Währungen::new(cacher.clone());
        let kursabfrage = Kursabfrage::new(cacher.clone());
        let meldungen = Meldungen::new(cacher, währungen.clone());
        Ok(Self {
            heute,
            meldungen,
            kursabfrage,
            währungen,
        })
    }

    pub async fn wertpapier_auswerten(
        &mut self,
        wertpapier: format::Wertpapier,
    ) -> Result<Wertpapier> {
        match wertpapier.typ {
            WertpapierTyp::Aktie => self.aktie_auswerten(wertpapier).await,
            WertpapierTyp::Etf => self.etf_auswerten(wertpapier).await,
            WertpapierTyp::Fond => {
                let wertpapier = self.aktie_auswerten(wertpapier).await?;
                self.fond_auswerten(wertpapier).await
            }
        }
    }

    pub async fn aktie_auswerten(&mut self, wertpapier: format::Wertpapier) -> Result<Wertpapier> {
        let format::Wertpapier {
            typ,
            mut name,
            isin,
            mut symbol,
            mut transaktionen,
            ..
        } = wertpapier;

        transaktionen.sort_by_key(|t| t.datum());

        let suche = symbol.as_deref().unwrap_or(isin.as_str());
        if let Some(metadaten) = self.kursabfrage.aktie_suchen(suche).await? {
            symbol = Some(metadaten.symbol);
            name = metadaten.name;
        }

        let mut jahre = vec![];
        let mut bestand = Bestand::default();

        for transaktion in transaktionen {
            let datum = transaktion.datum();

            let (bestand_, typ, steuer) = match transaktion {
                format::Transaktion::Kauf(_, format::Zahl(stück), format::Zahl(preis), _) => {
                    kauf_berechnen(bestand, stück, preis)
                }
                format::Transaktion::Verkauf(_, format::Zahl(stück), format::Zahl(preis), _) => {
                    verkauf_berechnen(bestand, stück, preis)
                }

                format::Transaktion::Split(_, format::Zahl(faktor)) => {
                    split_berechnen(bestand, faktor)
                }
                format::Transaktion::Ausgliederung(_, format::Zahl(faktor), andere_isin) => {
                    let eigener_kurs = self.kursabfrage.kurs_für_isin(&isin, datum).await?;
                    let eigener_kurs = self
                        .währungen
                        .kurs_in_euro(Kurs {
                            wert: eigener_kurs.open,
                            währung: eigener_kurs.währung,
                            datum,
                        })
                        .await?;
                    let anderer_kurs = self.kursabfrage.kurs_für_isin(&andere_isin, datum).await?;
                    let anderer_kurs = self
                        .währungen
                        .kurs_in_euro(Kurs {
                            wert: anderer_kurs.open,
                            währung: anderer_kurs.währung,
                            datum,
                        })
                        .await?;

                    ausgliederung_berechnen(
                        bestand,
                        faktor,
                        andere_isin,
                        eigener_kurs,
                        anderer_kurs,
                    )
                }
                format::Transaktion::Einbuchung(_, format::Zahl(stück)) => {
                    let symbol = symbol.as_deref().context("Aktie sollte ein Symbol haben")?;
                    let kurs = self.kursabfrage.kurs_abrufen(symbol, datum).await?;
                    let kurs = self
                        .währungen
                        .kurs_in_euro(Kurs {
                            wert: kurs.open,
                            währung: kurs.währung,
                            datum: kurs.datum,
                        })
                        .await?;
                    einbuchung_berechnen(bestand, stück, kurs)
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
                ) => dividende_berechnen(bestand, &isin, brutto, auszahlung),

                format::Transaktion::Ausschüttung(..) => {
                    anyhow::bail!("Aktien haben keine Ausschüttungen");
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
            symbol,
            jahre,
        })
    }

    pub async fn etf_auswerten(&mut self, wertpapier: format::Wertpapier) -> Result<Wertpapier> {
        let format::Wertpapier {
            typ,
            isin,
            symbol,
            mut transaktionen,
            ..
        } = wertpapier;

        transaktionen.sort_by_key(|t| t.datum());

        let meldungsdaten = self.meldungen.fetch_meldungen(&isin).await?;
        let name = meldungsdaten.name;

        let mut transaktionen = transaktionen.into_iter().peekable();
        let mut meldungen = meldungsdaten.meldungen.into_iter().peekable();

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
                    self.meldungen
                        .fetch_meldungs_details(nächste_meldung)
                        .await?;

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
                            dbg!(name, bestand, nächste_transaktion, nächste_meldung);
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
                format::Transaktion::Kauf(_, format::Zahl(stück), format::Zahl(preis), _) => {
                    kauf_berechnen(bestand, stück, preis)
                }
                format::Transaktion::Verkauf(_, format::Zahl(stück), format::Zahl(preis), _) => {
                    verkauf_berechnen(bestand, stück, preis)
                }

                format::Transaktion::Ausschüttung(_, format::Zahl(auszahlung)) => {
                    let meldung = meldung.take();
                    if let Some(meldung) = &meldung {
                        // wir nutzen hier das datum der meldung
                        // die tatsächliche auszahlung kann wegen wochenende usw verzögert sein
                        datum = meldung.datum;
                    }
                    ausschüttung_berechnen(bestand, auszahlung, meldung)
                }

                transaktion => {
                    anyhow::bail!("Transaktion `{transaktion:?}` bei ETF nicht möglich");
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
            symbol,
            jahre,
        })
    }

    pub async fn fond_auswerten(&mut self, mut wertpapier: Wertpapier) -> Result<Wertpapier> {
        let symbol = wertpapier
            .symbol
            .as_deref()
            .context("Fond sollte ein Symbol haben")?;

        let mut korrektur = Zahl::default();
        let mut wert_anfang = Zahl::default();
        for jahr in &mut wertpapier.jahre {
            jahr.bestand_anfang.preis += korrektur;

            for transaktion in &mut jahr.transaktionen {
                transaktion.bestand.preis += korrektur;
                match &transaktion.typ {
                    TransaktionsTyp::Kauf { stück, preis } => {
                        wert_anfang += stück * preis;
                    }
                    TransaktionsTyp::Verkauf { .. } => {
                        // TODO
                    }
                    TransaktionsTyp::Ausgliederung { .. } => {
                        todo!()
                    }
                    TransaktionsTyp::Einbuchung { .. } => {
                        todo!()
                    }
                    TransaktionsTyp::Spitzenverwertung { .. } => {
                        todo!()
                    }
                    TransaktionsTyp::Dividende { .. } => {
                        // transaktion.typ = TransaktionsTyp::Ausschüttung {
                        //     brutto: *brutto,
                        //     melde_id: None,
                        // }
                    }
                    TransaktionsTyp::Split { .. } => {}
                    TransaktionsTyp::Ausschüttung { .. }
                    | TransaktionsTyp::Jahresmeldung { .. } => todo!(),
                }
            }

            let datum = jahr.letzter();
            dbg!(datum, self.heute, korrektur);
            if datum > self.heute {
                continue;
            }
            let mut bestand = jahr.bestand_ende;
            if bestand.stück.is_zero() {
                wert_anfang = Zahl::default();
                continue;
            }

            // jahres schlusskurs abrufen
            let kurse = self.kursabfrage.kurse_abrufen(symbol, datum).await?;
            let idx = match kurse.binary_search_by_key(&datum, |daten| daten.datum) {
                Ok(idx) => idx,
                Err(0) => 0,
                Err(idx) => idx - 1,
            };
            let kurs = kurse.into_iter().nth(idx).unwrap();
            let kurs = self
                .währungen
                .kurs_in_euro(Kurs {
                    wert: kurs.close,
                    währung: kurs.währung,
                    datum: kurs.datum,
                })
                .await?;
            let wert_ende = bestand.stück * kurs;

            let (_korrektur, typ, steuer) =
                ausschüttungsgleiche_beträge_berechnen(bestand.stück, wert_anfang, wert_ende);
            bestand.preis += _korrektur;
            korrektur += _korrektur;
            let transaktion = Transaktion {
                datum,
                bestand,
                typ,
                steuer,
            };

            jahr.transaktionen.push(transaktion);
            wert_anfang = wert_ende;
            jahr.bestand_ende = bestand;
        }

        Ok(wertpapier)
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
        let mut rechner = Rechner::new().await.unwrap();

        let tencent = serde_yaml::from_str(
            r#"
typ: aktie
name: Tencent
isin: KYG875721634
transaktionen:
- kauf: [2023-01-01, 100, 100]
- ausgliederung: [2023-01-05, 1/10, KYG596691041]
        "#,
        )
        .unwrap();
        let tencent = rechner.wertpapier_auswerten(tencent).await.unwrap();
        dbg!(&tencent);

        let meituan = serde_yaml::from_str(
            r#"
typ: aktie
name: Meituan
isin: KYG596691041
transaktionen:
- einbuchung: [2023-01-05, 10]
        "#,
        )
        .unwrap();
        let meituan = rechner.wertpapier_auswerten(meituan).await.unwrap();
        dbg!(&meituan);
    }
}
