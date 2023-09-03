use std::fmt::{self, Arguments, Write};

use chrono::Datelike;
use fixed_decimal::{DoublePrecision, FixedDecimal};
use icu_decimal::FixedDecimalFormatter;
use icu_locid::{locale, Locale};
use writeable::Writeable;

use crate::data::{Bestand, Date, Number, Security, SecurityType, Steuern, TransactionKind};

const LOCALE: Locale = locale!("de-AT");

pub const WIDTH: usize = 80;

mod provider {
    // icu4x-datagen --keys-for-bin ... --locales de-at --format=mod --use-separate-crates --pretty --overwrite
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

    pub fn bestand(&mut self, stück: Number, preis: Number) -> &str {
        self.buffer.clear();
        self.fdf
            .format(&Self::stk_n(stück))
            .write_to(&mut self.buffer)
            .unwrap();
        self.buffer.push_str(" Stück");
        if preis > 0. {
            self.buffer.push_str(" × € ");
            self.fdf
                .format(&Self::eur_n(preis))
                .write_to(&mut self.buffer)
                .unwrap();
        }
        &self.buffer
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Writer<W: Write> {
    inner: W,
    buf: String,
}

impl<W: Write> Writer<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            buf: String::new(),
        }
    }
    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn divider(&mut self, ch: char) {
        for _ in 0..WIDTH {
            self.inner.write_char(ch).unwrap();
        }
        self.inner.write_char('\n').unwrap();
    }

    pub fn write_split(&mut self, left: &str, right: &str) {
        let pad = WIDTH - 1 - left.chars().count();
        writeln!(&mut self.inner, "{left} {right:>pad$}").unwrap();
    }
    fn write_split_fmt(&mut self, left: Arguments, right: &str) {
        self.buf.clear();
        self.buf.write_fmt(left).unwrap();

        let pad = WIDTH - 1 - self.buf.chars().count();
        writeln!(&mut self.inner, "{} {right:>pad$}", self.buf).unwrap();
    }
}

impl<W: Write> Write for Writer<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.inner.write_str(s)
    }
}

pub fn write_and_sum_report<W: Write>(
    w: &mut Writer<W>,
    security: &Security,
    year: Option<i32>,
    print_sum: bool,
) -> (Number, Steuern) {
    let mut f = Formatter::new();
    let typ = match security.typ {
        SecurityType::Etf => "ETF",
        SecurityType::Aktie => "Aktie",
    };
    writeln!(w, "ISIN: {} ({typ})", security.isin).unwrap();
    writeln!(w, "{}", security.name).unwrap();

    let mut devider = '=';

    let mut bestand = Bestand::default();
    let mut steuern_gesamt = Steuern::default();
    let mut auszahlung_gesamt = 0.0;

    let mut transaktionen = security.transaktionen.iter().peekable();
    let date_end = if let Some(year) = year {
        let date_start = Date::from_ymd_opt(year, 1, 1).unwrap();
        let date_end = Date::from_ymd_opt(year, 12, 31).unwrap();

        while let Some(transaktion) = transaktionen.peek() {
            if transaktion.datum >= date_start {
                break;
            }
            bestand = transaktion.bestand.clone();
            transaktionen.next();
        }

        w.divider('=');
        devider = '-';
        w.write_split_fmt(
            format_args!("Bestand am {date_start}:"),
            f.bestand(bestand.stück, bestand.preis),
        );
        date_end
    } else {
        chrono::Local::now().date_naive()
    };
    let mut last_date: Option<Date> = None;

    for transaktion in transaktionen {
        let datum = transaktion.datum;
        if datum >= date_end {
            break;
        }

        let changes_year = last_date.map(|d| d.year() != datum.year()).unwrap_or(false);
        if changes_year {
            w.divider(devider);
            devider = '-';

            let date = Date::from_ymd_opt(datum.year(), 1, 1).unwrap();
            w.write_split_fmt(
                format_args!("Bestand am {date}:"),
                f.bestand(bestand.stück, bestand.preis),
            );
        }
        last_date = Some(datum);

        bestand = transaktion.bestand.clone();
        let steuer = &transaktion.steuern;
        steuern_gesamt += steuer;

        w.divider(devider);
        devider = '-';
        match transaktion.typ {
            TransactionKind::Kauf { stück, preis } => {
                writeln!(w, "Kauf am {datum}: {}", f.bestand(stück, preis)).unwrap();
                w.write_split("Neuer Bestand:", f.bestand(bestand.stück, bestand.preis));
            }
            TransactionKind::Verkauf { stück, preis } => {
                writeln!(w, "Verkauf am {datum}: {}", f.bestand(stück, preis)).unwrap();
                print_steuern(w, &mut f, steuer);
                w.write_split("Neuer Bestand:", f.bestand(bestand.stück, bestand.preis));
                auszahlung_gesamt += stück * preis;
            }
            TransactionKind::Split { faktor } => {
                writeln!(w, "Aktiensplit am {datum} mit Faktor {faktor}").unwrap();
                w.write_split("Neuer Bestand:", f.bestand(bestand.stück, bestand.preis));
            }
            TransactionKind::Dividende { auszahlung, .. } => {
                writeln!(w, "Dividendenzahlung am {datum}:").unwrap();
                writeln!(w, "Auszahlung: {}", f.eur(auszahlung)).unwrap();
                print_steuern(w, &mut f, steuer);
                auszahlung_gesamt += auszahlung;
            }
            TransactionKind::Ausschüttung { brutto, melde_id } => {
                if melde_id > 0 {
                    writeln!(w, "Ausschüttung mit Meldung am {datum} (Id: {melde_id})",).unwrap();
                } else {
                    writeln!(w, "Ausschüttung ohne Meldung am {datum}:").unwrap();
                }
                writeln!(w, "Auszahlung: {}", f.eur(brutto)).unwrap();
                print_steuern(w, &mut f, steuer);
                if melde_id > 0 {
                    w.write_split("Neuer Bestand:", f.bestand(bestand.stück, bestand.preis));
                }
                auszahlung_gesamt += brutto;
            }
            TransactionKind::Jahresmeldung { melde_id } => {
                writeln!(w, "Jahresmeldung am {datum} (Id: {melde_id}):").unwrap();
                print_steuern(w, &mut f, steuer);
                w.write_split("Neuer Bestand:", f.bestand(bestand.stück, bestand.preis));
            }
        }
    }

    w.divider('-');
    w.write_split_fmt(
        format_args!("Bestand am {date_end}:"),
        f.bestand(bestand.stück, bestand.preis),
    );

    w.divider('=');
    writeln!(w, "Summe:").unwrap();
    print_steuern(w, &mut f, &steuern_gesamt);

    if print_sum {
        writeln!(w).unwrap();
        w.write_split("Auszahlung gesamt:", f.eur(auszahlung_gesamt));
        let nachzahlung = steuern_gesamt.nachzahlung();
        w.write_split("Steuernachzahlung:", f.eur(nachzahlung));
    }

    (auszahlung_gesamt, steuern_gesamt)
}

pub fn print_steuern<W: Write>(w: &mut Writer<W>, f: &mut Formatter, steuer: &Steuern) {
    if steuer.dividendenerträge_863 > 0. {
        w.write_split(
            "Einkünfte aus Dividenden (863):",
            f.eur(steuer.dividendenerträge_863),
        );
    }
    if steuer.wertsteigerungen_994 > 0. {
        w.write_split(
            "Einkünfte aus realisierten Wertsteigerungen (994):",
            f.eur(steuer.wertsteigerungen_994),
        );
    }
    if steuer.wertverluste_892 > 0. {
        w.write_split(
            "Verluste aus realisierten Wertverlusten (892):",
            f.eur(steuer.wertverluste_892),
        );
    }
    if steuer.ausschüttungen_898 > 0. {
        w.write_split("Ausschüttungen (898):", f.eur(steuer.ausschüttungen_898));
    }
    if steuer.ausschüttungsgleiche_erträge_937 > 0. {
        w.write_split(
            "Ausschüttungsgleiche Erträge (937):",
            f.eur(steuer.ausschüttungsgleiche_erträge_937),
        );
    }
    if steuer.gezahlte_kest_899 > 0. {
        w.write_split("Gezahlte KeSt (899):", f.eur(steuer.gezahlte_kest_899));
    }
    if steuer.anrechenbare_quellensteuer_998 > 0. {
        w.write_split(
            "Anrechenbare Quellensteuer (998):",
            f.eur(steuer.anrechenbare_quellensteuer_998),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::scraper::Scraper;
    use crate::taxes::do_taxes;

    use super::*;

    #[tokio::test]
    async fn test_print_report() {
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
        let mut scraper = Scraper::new();
        do_taxes(&mut scraper, &mut security).await;

        let mut writer = Writer::new(String::new());
        let sum = write_and_sum_report(&mut writer, &security, Some(2023), true);
        println!("{}", writer.into_inner());
        dbg!(sum);
    }

    #[tokio::test]
    async fn test_etf_report() {
        let mut security: Security = serde_yaml::from_str(
            r#"
typ: etf
name: Foo
isin: IE00B9CQXS71
transaktionen:
- kauf: [2020-01-01, 10, 10]
        "#,
        )
        .unwrap();
        let mut scraper = Scraper::new();
        do_taxes(&mut scraper, &mut security).await;

        let mut writer = Writer::new(String::new());
        let sum = write_and_sum_report(&mut writer, &security, None, true);
        println!("{}", writer.into_inner());
        dbg!(sum);
    }
}
