extern crate alloc; // because of `BakedDataProvider`

use anyhow::Result;

pub mod data;
pub mod report;
pub mod scraper;
pub mod taxes;

#[tokio::main]
async fn main() -> Result<()> {
    // let report = scraper::fetch_reports("IE00B9M6RS56").await?;
    // // dbg!(&report);

    // println!("ISIN: {}", report.isin);
    // println!("Name: {}", report.name);
    // println!("===");

    // println!("Datum;MeldungsID;Währung;Kurs;Jahresmeldung;StB_E1KV_Ausschuettungen;StB_E1KV_AGErtraege;StB_E1KV_anzurechnende_ausl_Quellensteuer;StB_E1KV_Korrekturbetrag_saldiert");

    // for row in report.rows {
    //     let printed = format!(
    //         "{};{};{};{};{};{};{};{};{}",
    //         row.date,
    //         row.report_id,
    //         row.currency,
    //         row.rate,
    //         row.is_yearly_report,
    //         row.StB_E1KV_Ausschuettungen,
    //         row.StB_E1KV_AGErtraege,
    //         row.StB_E1KV_anzurechnende_ausl_Quellensteuer,
    //         row.StB_E1KV_Korrekturbetrag_saldiert
    //     );
    //     // dat formatting -_-
    //     let with_comma = printed.replace('.', ",");
    //     println!("{with_comma}");
    // }

    Ok(())
}
