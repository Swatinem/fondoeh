use fixed_decimal::{DoublePrecision, FixedDecimal};
use icu::decimal::FixedDecimalFormatter;
use icu::locid::{locale, Locale};
use writeable::Writeable;

use crate::data::{Bestand, Date, Number, Security, SecurityType};

const LOCALE: Locale = locale!("de-AT");

const WIDTH: usize = 70;

mod provider {
    // icu4x-datagen --keys-for-bin ... --locales de-at --format=mod --pretty --overwrite
    include!("../icu4x_data/mod.rs");
}

pub struct Formatter {
    buffer: String,
    fdf: FixedDecimalFormatter,
}

impl Formatter {
    pub fn new() -> Self {
        let buffer = String::new();
        let fdf = FixedDecimalFormatter::try_new_unstable(
            &provider::BakedDataProvider,
            &LOCALE.into(),
            Default::default(),
        )
        .unwrap();
        Self { buffer, fdf }
    }

    pub fn stk_n(num: Number) -> FixedDecimal {
        FixedDecimal::try_from_f64(num, DoublePrecision::Magnitude(-4))
            .unwrap()
            .trimmed_end()
    }
    pub fn eur_n(num: Number) -> FixedDecimal {
        FixedDecimal::try_from_f64(num, DoublePrecision::Magnitude(-4))
            .unwrap()
            .trimmed_end()
            .padded_end(-2)
    }

    pub fn stk(&mut self, num: Number) -> &str {
        self.buffer.clear();
        self.fdf
            .format(&Self::stk_n(num))
            .write_to(&mut self.buffer)
            .unwrap();
        &self.buffer
    }

    pub fn eur(&mut self, num: Number) -> &str {
        self.buffer.clear();
        self.buffer.push_str("€ ");
        self.fdf
            .format(&Self::eur_n(num))
            .write_to(&mut self.buffer)
            .unwrap();
        &self.buffer
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn print_report(security: &Security, year: i32) {
    let date_start = Date::from_ymd_opt(year, 1, 1).unwrap();
    let date_end = Date::from_ymd_opt(year, 12, 31).unwrap();

    let mut transaktionen = security.transaktionen.iter().peekable();

    let mut f = Formatter::new();
    let mut bestand = Bestand::default();

    while let Some(transaktion) = transaktionen.peek() {
        bestand = transaktion.bestand.clone();
        if transaktion.datum >= date_start {
            break;
        }
    }

    let typ = match security.typ {
        SecurityType::Etf => "ETF",
        SecurityType::Aktie => "Aktie",
    };
    println!("ISIN: {} ({typ})", security.isin);
    println!("{}", security.name);
    println!("{:-<WIDTH$}", "");
    println!(
        "Bestand am {date_start}: {} Stück × {}",
        f.stk(bestand.stück).to_owned(), // FIXME
        f.eur(bestand.preis)
    );

    for transaktion in transaktionen {
        let datum = transaktion.datum;
        if datum >= date_end {
            break;
        }
        bestand = transaktion.bestand.clone();
        let steuer = &transaktion.steuern;

        println!("{:-<WIDTH$}", "");
        match transaktion.typ {
            crate::data::TransactionKind::Kauf { stück, preis } => {
                println!(
                    "Kauf am {datum}: {} Stück × {}",
                    f.stk(stück).to_owned(), // FIXME
                    f.eur(preis)
                );
                println!(
                    "Neuer Bestand: {} Stück × {}",
                    f.stk(bestand.stück).to_owned(), // FIXME
                    f.eur(bestand.preis)
                );
            }
            crate::data::TransactionKind::Verkauf { stück, preis } => {
                println!(
                    "Verkauf am {datum}: {} Stück × {}",
                    f.stk(stück).to_owned(), // FIXME
                    f.eur(preis)
                );
                println!(
                    "Einkünfte aus realisierten Wertsteigerungen (994): {}",
                    f.eur(steuer.gewinn)
                );
                println!(
                    "Neuer Bestand: {} Stück × {}",
                    f.stk(bestand.stück).to_owned(), // FIXME
                    f.eur(bestand.preis)
                );
            }
            crate::data::TransactionKind::Split { faktor } => {
                println!("Aktiensplit am {datum} mit Faktor {faktor}");
                println!(
                    "Neuer Bestand: {} Stück × {}",
                    f.stk(bestand.stück).to_owned(), // FIXME
                    f.eur(bestand.preis)
                );
            }
            crate::data::TransactionKind::Dividende { brutto, ertrag } => {
                println!("Dividendenzahlung am {datum}:");
                println!("Bruttodividende: {}", f.eur(brutto));
                println!("Auszahlung: {}", f.eur(ertrag));
                println!(
                    "Anrechenbare Quellensteuer (998): {}",
                    f.eur(steuer.anrechenbare_quellensteuer)
                );
            }
            crate::data::TransactionKind::Ausschüttung { brutto } => todo!(),
        }
    }

    println!("{:-<WIDTH$}", "");
    println!(
        "Bestand am {date_end}: {} Stück × {}",
        bestand.stück, bestand.preis
    );

    // TODO: summe aller steuern
}

#[cfg(test)]
mod tests {
    use crate::taxes::do_taxes;

    use super::*;

    #[test]
    fn test_print_report() {
        let mut security: Security = serde_yaml::from_str(
            r#"
typ: aktie
name: Foo
isin: DE000
transaktionen:
- kauf: [2023-01-01, 40, 30.23]
- split: [2023-02-01, 3]
- dividende: [2023-03-01, 100, 85]
- verkauf: [2023-04-01, 60, 15]
- kauf: [2023-05-01, 60, 10]
        "#,
        )
        .unwrap();
        do_taxes(&mut security);

        print_report(&security, 2023);
    }
}
