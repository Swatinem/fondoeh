use std::borrow::Cow;
use std::fmt;

use anyhow::{Context, Result};
pub use chrono::naive::NaiveDate as Datum;
pub use num_rational::Rational64;
use serde::Deserialize;
pub use smol_str::SmolStr as String;

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum WertpapierTyp {
    Etf,
    Aktie,
}

impl fmt::Display for WertpapierTyp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WertpapierTyp::Etf => "ETF",
            WertpapierTyp::Aktie => "Aktie",
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Wertpapier {
    pub typ: WertpapierTyp,
    pub name: String,
    pub isin: String,
    pub symbol: Option<String>,
    pub transaktionen: Vec<Transaktion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transaktion {
    Kauf(Datum, Zahl, Zahl),
    Verkauf(Datum, Zahl, Zahl),

    Split(Datum, Zahl),
    Ausgliederung(Datum, Zahl, String),
    Einbuchung(Datum, Zahl),
    Spitzenverwertung(Datum, Zahl, Zahl),

    Dividende(Datum, Zahl, Zahl),
    Ausschüttung(Datum, Zahl),
}

impl Transaktion {
    pub fn datum(&self) -> Datum {
        match self {
            Transaktion::Kauf(datum, _, _) => *datum,
            Transaktion::Verkauf(datum, _, _) => *datum,
            Transaktion::Spitzenverwertung(datum, _, _) => *datum,
            Transaktion::Ausgliederung(datum, _, _) => *datum,
            Transaktion::Einbuchung(datum, _) => *datum,
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
    let (vor, vorzeichen) = if let Some(vor) = vor.strip_prefix('-') {
        (vor, -1)
    } else {
        (vor, 1)
    };
    let zahl = Rational64::new(vor.parse()?, 1);

    let Some(nach) = split.next() else {
        return Ok(zahl * vorzeichen);
    };

    let faktor = 10_i64.pow(nach.len() as u32);
    let nach = Rational64::new(nach.parse()?, 1);

    Ok((zahl * faktor + nach) / faktor * vorzeichen)
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
