extern crate alloc; // because of `BakedDataProvider`

use std::fmt::Write;
use std::fs::File;

use anyhow::{Context, Result};
use chrono::Datelike;

use data::{Date, Steuern, TransactionKind};
use report::{print_steuern, write_and_sum_report, Formatter, Writer, WIDTH};
use scraper::Scraper;
use taxes::do_taxes;

use crate::data::Transaction;

pub mod data;
pub mod deser;
pub mod report;
pub mod scraper;
pub mod taxes;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = args.next().context("needs directory")?;

    let mut scraper = Scraper::new();
    let mut reports = vec![];
    let mut auszahlung_gesamt = 0.0;
    let mut steuer_gesamt = Steuern::default();

    let heute = chrono::Local::now().date_naive();

    let year = if let Some(year) = args.next() {
        Some(year.parse()?)
    } else {
        None
    };
    let files = glob::glob(&format!("{dir}/**/*.yaml"))?;
    for file in files {
        let file = file?;
        let rdr = File::open(&file)?;

        let mut security =
            serde_yaml::from_reader(rdr).context(format!("failed parsing {}", file.display()))?;

        do_taxes(&mut scraper, &mut security, heute.year()).await;

        let mut writer = Writer::new(String::new());
        if let Some(summe) = write_and_sum_report(&mut writer, &security, year, false) {
            auszahlung_gesamt += summe.0;
            steuer_gesamt += &summe.1;
        };

        let s = writer.into_inner();

        reports.push((security, s));
    }

    reports.sort_by(|a, b| (&a.0.typ, &a.0.isin).cmp(&(&b.0.typ, &b.0.isin)));

    let mut f = Formatter::new();

    print!("Name\tISIN\tArt\tDatum\tBestand\tDurchschnittspreis\t"); // 6
    print!("Aktion\tStück\tPreis\tBrutto\tAuszahlung\tÜberschuss (994)\tVerlust (892)\t"); // 7
    print!("Dividendenertrag (863)\tGezahlte KeSt (899)\tAnrechenbare Quellensteuer (998)\t"); // 3
    println!("Ausschüttung (898)\tAusschüttungsgl Ertrag (937)\tMelde-ID"); // 3

    for (security, _) in reports {
        let jahr = 2022;
        // dbg!(jahr, &security);

        let Some(daten_jahr) = security.jahre.get(jahr) else {
            continue;
        };
        // dbg!(daten_jahr);
        let erster = Date::from_ymd_opt(jahr, 1, 1).unwrap();
        let bestand = &daten_jahr.bestand_anfang;
        println!(
            "{}\t{}\t{:?}\t{}\t{}\t{}\tBestand",
            security.name,
            security.isin,
            security.typ,
            erster,
            bestand.stück,
            f.eur(bestand.preis),
        );

        for transaktion in &daten_jahr.transaktionen {
            let Transaction {
                bestand, steuern, ..
            } = transaktion;
            print!(
                "{}\t{}\t{:?}\t{}\t{}\t{}\t",
                security.name,
                security.isin,
                security.typ,
                transaktion.datum,
                bestand.stück,
                f.eur(bestand.preis),
            );
            match transaktion.typ {
                TransactionKind::Kauf { stück, preis } => println!("Kauf\t{}\t{}", stück, preis),
                TransactionKind::Verkauf { stück, preis } => {
                    println!(
                        "Verkauf\t{}\t{}\t{}\t{}\t{}\t{}",
                        stück,
                        f.eur(preis).to_owned(),
                        f.eur(stück * preis).to_owned(),
                        f.eur(stück * preis).to_owned(),
                        f.eur(steuern.wertsteigerungen_994).to_owned(),
                        f.eur(steuern.wertverluste_892)
                    )
                }
                TransactionKind::Split { faktor } => println!("Split\t{}", faktor),
                TransactionKind::Dividende { brutto, auszahlung } => {
                    println!(
                        "Dividende\t\t\t{}\t{}\t\t\t{}\t{}\t{}",
                        f.eur(brutto).to_owned(),
                        f.eur(auszahlung).to_owned(),
                        f.eur(steuern.dividendenerträge_863).to_owned(),
                        f.eur(steuern.gezahlte_kest_899).to_owned(),
                        f.eur(steuern.anrechenbare_quellensteuer_998)
                    )
                }
                TransactionKind::Ausschüttung { brutto, melde_id } => {
                    if melde_id == 0 {
                        println!(
                            "Ausschüttung ohne Meldung\t\t\t{}\t{}\t\t\t\t\t\t{}",
                            f.eur(brutto).to_owned(),
                            f.eur(brutto).to_owned(),
                            f.eur(steuern.ausschüttungen_898)
                        )
                    } else {
                        println!(
                            "Ausschüttung mit Meldung\t\t\t{}\t{}\t\t\t\t\t{}\t{}\t{}\t{}",
                            f.eur(brutto).to_owned(),
                            f.eur(brutto).to_owned(),
                            f.eur(steuern.anrechenbare_quellensteuer_998).to_owned(),
                            f.eur(steuern.ausschüttungen_898).to_owned(),
                            f.eur(steuern.ausschüttungsgleiche_erträge_937),
                            melde_id
                        )
                    }
                }
                TransactionKind::Jahresmeldung { melde_id } => println!(
                    "Jahresmeldung\t\t\t\t\t\t\t\t\t{}\t{}\t{}\t{}",
                    f.eur(steuern.anrechenbare_quellensteuer_998).to_owned(),
                    f.eur(steuern.ausschüttungen_898).to_owned(),
                    f.eur(steuern.ausschüttungsgleiche_erträge_937),
                    melde_id
                ),
            }
        }

        let letzter = Date::from_ymd_opt(jahr, 12, 31).unwrap();
        let bestand = &daten_jahr.bestand_ende;
        println!(
            "{}\t{}\t{:?}\t{}\t{}\t{}\tBestand",
            security.name,
            security.isin,
            security.typ,
            letzter,
            bestand.stück,
            f.eur(bestand.preis),
        );
    }

    return Ok(());

    let mut writer = Writer::new(String::new());
    let w = &mut writer;
    print_steuern(w, &mut f, &steuer_gesamt);
    writeln!(w).unwrap();
    w.write_split("Auszahlung gesamt:", f.eur(auszahlung_gesamt));
    let nachzahlung = steuer_gesamt.nachzahlung();
    w.write_split("Steuernachzahlung:", f.eur(nachzahlung));
    // let ertrag = auszahlung_gesamt - nachzahlung;
    // w.write_split("TODO: Ertrag effektiv", formatter.eur(ertrag));
    // if year.is_some() {
    //     w.write_split("Ertrag effektiv pro Monat", formatter.eur(ertrag / 12.));
    // }

    println!("{}", writer.into_inner());

    for report in reports {
        println!("{:#<WIDTH$}", "");
        println!();
        println!("{}", report.1);
    }

    Ok(())
}
