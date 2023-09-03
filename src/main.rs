extern crate alloc; // because of `BakedDataProvider`

use std::fmt::Write;
use std::fs::File;

use anyhow::{Context, Result};

use data::Steuern;
use report::{print_steuern, write_and_sum_report, Formatter, Writer, WIDTH};
use scraper::Scraper;
use taxes::do_taxes;

pub mod data;
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

        do_taxes(&mut scraper, &mut security).await;

        let mut writer = Writer::new(String::new());
        let Some(summe) = write_and_sum_report(&mut writer, &security, year, false) else {
            continue;
        };
        auszahlung_gesamt += summe.0;
        steuer_gesamt += &summe.1;

        let s = writer.into_inner();

        reports.push((security, s));
    }

    reports.sort_by(|a, b| (&a.0.typ, &a.0.isin).cmp(&(&b.0.typ, &b.0.isin)));

    let mut writer = Writer::new(String::new());
    let w = &mut writer;
    let mut formatter = Formatter::new();
    print_steuern(w, &mut formatter, &steuer_gesamt);
    writeln!(w).unwrap();
    w.write_split("Auszahlung gesamt:", formatter.eur(auszahlung_gesamt));
    let nachzahlung = steuer_gesamt.nachzahlung();
    w.write_split("Steuernachzahlung:", formatter.eur(nachzahlung));
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
