use anyhow::Result;
use chrono::Datelike;
use num_traits::identities::Zero;

use crate::scraper::Scraper;
use crate::{format, Jahr, Transaktion};
use crate::{Datum, Wertpapier};

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
            name,
            isin,
            mut transaktionen,
        } = wertpapier;

        transaktionen.sort_by_key(|t| t.datum());
        let mut transaktionen = transaktionen.into_iter().peekable();

        let mut jahre = vec![];

        loop {
            let Some(mut transaktion) = transaktionen.next() else {
                break;
            };
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
