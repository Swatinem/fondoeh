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
    Dividende { brutto: Number, auszahlung: Number },
    Ausschüttung { brutto: Number, melde_id: usize },
    Jahresmeldung { melde_id: usize },
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
    pub dividendenerträge_863: Number,
    pub wertsteigerungen_994: Number,
    pub wertverluste_892: Number,
    pub ausschüttungen_898: Number,
    pub ausschüttungsgleiche_erträge_937: Number,
    pub gezahlte_kest_899: Number,
    pub anrechenbare_quellensteuer_998: Number,
}

impl std::ops::AddAssign<&Self> for Steuern {
    fn add_assign(&mut self, rhs: &Self) {
        self.dividendenerträge_863 += rhs.dividendenerträge_863;
        self.wertsteigerungen_994 += rhs.wertsteigerungen_994;
        self.wertverluste_892 += rhs.wertverluste_892;
        self.ausschüttungen_898 += rhs.ausschüttungen_898;
        self.ausschüttungsgleiche_erträge_937 += rhs.ausschüttungsgleiche_erträge_937;
        self.gezahlte_kest_899 += rhs.gezahlte_kest_899;
        self.anrechenbare_quellensteuer_998 += rhs.anrechenbare_quellensteuer_998;
    }
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
                typ: TransactionKind::Dividende {
                    brutto,
                    auszahlung: ertrag,
                },
                bestand,
                steuern,
            },
            RawTransaction::Ausschüttung(datum, brutto) => Self {
                datum,
                typ: TransactionKind::Ausschüttung {
                    brutto,
                    melde_id: 0,
                },
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
