pub use chrono::naive::NaiveDate as Date;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SecurityType {
    Etf,
    Aktie,
}

#[derive(Debug, Deserialize)]
pub struct Security {
    pub typ: SecurityType,
    pub name: String,
    pub isin: String,
    pub transaktionen: Vec<Transaction>,
}

#[derive(Debug, Deserialize)]
#[serde(from = "RawTransaction")]
pub struct Transaction {
    pub datum: Date,
    pub typ: TransactionKind,
    pub bestand: Bestand,
    pub steuern: Steuern,
}

pub type Number = f64;

#[derive(Debug)]
pub enum TransactionKind {
    Kauf { stück: Number, preis: Number },
    Verkauf { stück: Number, preis: Number },
    Split { faktor: Number },
    Dividende { brutto: Number, ertrag: Number },
    Ausschüttung { brutto: Number },
}

#[derive(Debug, Clone, Default)]
pub struct Bestand {
    pub stück: Number,
    pub preis: Number,
}

impl Bestand {
    pub fn summe(&self) -> f64 {
        self.stück * self.preis
    }
}

#[derive(Debug, Default)]
pub struct Steuern {
    pub erlös: Number,
    pub gewinn: Number,
    pub anrechenbare_quellensteuer: Number,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RawTransaction {
    Kauf(Date, Number, Number),
    Verkauf(Date, Number, Number),
    Split(Date, Number),
    Dividende(Date, Number, Number),
    Ausschüttung(Date, Number),
}

impl From<RawTransaction> for Transaction {
    fn from(raw: RawTransaction) -> Self {
        let bestand = Bestand::default();
        let steuern = Steuern::default();
        match raw {
            RawTransaction::Kauf(datum, stück, preis) => Self {
                datum,
                typ: TransactionKind::Kauf { stück, preis },
                bestand,
                steuern,
            },
            RawTransaction::Verkauf(datum, stück, preis) => Self {
                datum,
                typ: TransactionKind::Verkauf { stück, preis },
                bestand,
                steuern,
            },
            RawTransaction::Split(datum, faktor) => Self {
                datum,
                typ: TransactionKind::Split { faktor },
                bestand,
                steuern,
            },
            RawTransaction::Dividende(datum, brutto, ertrag) => Self {
                datum,
                typ: TransactionKind::Dividende { brutto, ertrag },
                bestand,
                steuern,
            },
            RawTransaction::Ausschüttung(datum, brutto) => Self {
                datum,
                typ: TransactionKind::Ausschüttung { brutto },
                bestand,
                steuern,
            },
        }
    }
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
- kauf: [2023-01-01, 40, 30.23]
- ausschüttung: [2023-01-15, 1.23]
- verkauf: [2023-02-02, 40, 32]
        "#;
        let security: Security = serde_yaml::from_str(contents).unwrap();
        dbg!(security);
    }
}
