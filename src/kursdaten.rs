use anyhow::{Context, Result};
use chrono::Days;

use crate::{Datum, Zahl};

const SEARCH_BASE: &str =
    "https://query2.finance.yahoo.com/v1/finance/search?quotesCount=5&newsCount=0&q=";
const CHART_BASE: &str = "https://query1.finance.yahoo.com/v8/finance/chart/";

const BÖRSEN: &[&str] = &["GER", "FRA", "STU"];

#[derive(Debug)]
pub struct Metadaten {
    pub symbol: String,
    pub name: String,
}

#[derive(Debug)]
pub struct Kursdaten {
    client: reqwest::Client,
}

impl Kursdaten {
    pub fn new() -> Self {
        let client = reqwest::Client::new();

        Self { client }
    }
}

impl Kursdaten {
    pub async fn aktie_suchen(&self, isin: &str) -> Result<Metadaten> {
        let list_url = format!("{SEARCH_BASE}{isin}");
        let list = self.client.get(list_url).send().await?;

        let list: raw::Search = list.json().await.context("Aktie suchen")?;

        let mut aktien: Vec<_> = list
            .quotes
            .into_iter()
            .filter_map(|suche| {
                let idx = BÖRSEN
                    .iter()
                    .enumerate()
                    .find_map(|(idx, b)| (**b == suche.exchange).then_some(idx))?;
                Some((idx, suche))
            })
            .collect();
        aktien.sort_by_key(|aktien| aktien.0);

        let (_idx, aktie) = aktien
            .into_iter()
            .next()
            .context("Aktie sollte gefunden werden")?;

        let name = aktie.longname.unwrap_or(aktie.shortname);

        Ok(Metadaten {
            symbol: aktie.symbol,
            name,
        })
    }

    pub async fn kurs_abrufen(&self, symbol: &str, datum: Datum) -> Result<Zahl> {
        let vorher = (datum - Days::new(1)).and_hms_opt(0, 0, 0).unwrap();
        let nachher = (datum + Days::new(2)).and_hms_opt(0, 0, 0).unwrap();

        let chart_url = format!(
            "{CHART_BASE}{symbol}?interval=1d&period1={}&period2={}",
            vorher.timestamp(),
            nachher.timestamp()
        );
        let chart = self.client.get(chart_url).send().await?;
        let chart: raw::Chart = chart.json().await.context("Kursdaten abrufen")?;

        let result = chart
            .chart
            .result
            .into_iter()
            .next()
            .context("Kursdaten abfragen sollte ein Ergebnis liefern")?;

        // let erster_timestamp = result
        //     .timestamp
        //     .into_iter()
        //     .next()
        //     .context("Es sollte ein Kurs (Zeitpunkt) existieren")?;
        // dbg!(NaiveDateTime::from_timestamp_opt(erster_timestamp, 0));

        let erster_kurs = result
            .indicators
            .quote
            .into_iter()
            .next()
            .context("Es sollte ein Chart existieren")?;
        let erster_kurs = erster_kurs
            .open
            .into_iter()
            .next()
            .context("Es sollte ein Kurs existieren")?;

        let kurs = (erster_kurs * 10_000.) as i64;
        let kurs = Zahl::new(kurs, 10_000);
        Ok(kurs)
    }
}

mod raw {
    #[derive(Debug, serde::Deserialize)]
    pub struct Search {
        pub quotes: Vec<SearchQuote>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct SearchQuote {
        pub exchange: String,
        pub symbol: String,
        pub shortname: String,
        pub longname: Option<String>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct Chart {
        pub chart: ChartChart,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct ChartChart {
        pub result: Vec<ChartResult>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct ChartResult {
        pub meta: ChartMeta,
        pub timestamp: Vec<i64>,
        pub indicators: ChartIndicators,
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChartMeta {
        pub currency: String,
        pub symbol: String,
        pub exchange_name: String,
        pub first_trade_date: i64,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct ChartIndicators {
        pub quote: Vec<ChartQuote>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct ChartQuote {
        pub open: Vec<f64>,
    }
}

impl Default for Kursdaten {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn aktien_suchen() {
        let kursdaten = Kursdaten::new();
        let siemens = kursdaten.aktie_suchen("DE0007236101").await.unwrap();
        dbg!(&siemens);
        let siemens_energy = kursdaten.aktie_suchen("DE000ENER6Y0").await.unwrap();
        dbg!(&siemens_energy);

        let datum = Datum::from_ymd_opt(2020, 9, 28).unwrap();

        let kurs = kursdaten
            .kurs_abrufen(&siemens.symbol, datum)
            .await
            .unwrap();
        dbg!(kurs);

        let kurs = kursdaten
            .kurs_abrufen(&siemens_energy.symbol, datum)
            .await
            .unwrap();
        dbg!(kurs);
    }
}
