use std::borrow::Cow;

use anyhow::{Context, Result};
pub use chrono::naive::NaiveDate as Datum;
pub use num_rational::Rational64;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum WertpapierTyp {
    Etf,
    Aktie,
}

#[derive(Debug, Deserialize)]
pub struct Wertpapier {
    pub typ: WertpapierTyp,
    pub name: String,
    pub isin: String,
    pub transaktionen: Vec<Transaktion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transaktion {
    Kauf(Datum, Zahl, Zahl),
    Verkauf(Datum, Zahl, Zahl),
    Split(Datum, Zahl),
    Dividende(Datum, Zahl, Zahl),
    Ausschüttung(Datum, Zahl),
}

impl Transaktion {
    pub fn datum(&self) -> Datum {
        match self {
            Transaktion::Kauf(datum, _, _) => *datum,
            Transaktion::Verkauf(datum, _, _) => *datum,
            Transaktion::Split(datum, _) => *datum,
            Transaktion::Dividende(datum, _, _) => *datum,
            Transaktion::Ausschüttung(datum, _) => *datum,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "Cow<'_, str>")]
pub struct Zahl(pub Rational64);

impl TryFrom<Cow<'_, str>> for Zahl {
    type Error = anyhow::Error;

    fn try_from(s: Cow<'_, str>) -> Result<Self> {
        let mut split = s.trim().splitn(2, '/');
        let nenner = split.next().context("Zahl erwartet")?;
        let nenner = parse_kommazahl(nenner)?;

        let Some(zähler) = split.next() else {
            return Ok(Zahl(nenner));
        };

        let zähler = parse_kommazahl(zähler)?;
        Ok(Zahl(nenner / zähler))
    }
}

fn parse_kommazahl(s: &str) -> Result<Rational64> {
    let mut split = s.trim().splitn(2, '.');
    let vor = split.next().context("Zahl erwartet")?;
    let zahl = Rational64::new(vor.parse()?, 1);

    let Some(nach) = split.next() else {
        return Ok(zahl);
    };

    let faktor = 10_i64.pow(nach.len() as u32);
    let nach = Rational64::new(nach.parse()?, 1);

    Ok((zahl * faktor + nach) / faktor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example() {
        let contents = r#"
typ: etf
name: Foo
isin: DE000
transaktionen:
- kauf: [2023-01-01, 40, 30.023]
- ausschüttung: [2023-01-15, 1.23]
- split: [2023-02-02, 1/3]
- split: [2023-03-03, 3]
- verkauf: [2023-04-04, 40, 32]
        "#;
        let wertpapier: Wertpapier = serde_yaml::from_str(contents).unwrap();
        dbg!(wertpapier);
    }
}