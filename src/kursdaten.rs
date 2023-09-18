use anyhow::{Context, Result};
use chrono::{Days, NaiveDateTime};

use crate::cache::Cache;
use crate::{zahl_aus_float, Datum, String, Zahl};

const SEARCH_BASE: &str =
    "https://query2.finance.yahoo.com/v1/finance/search?quotesCount=5&newsCount=0&listsCount=0&q=";
const CHART_BASE: &str = "https://query1.finance.yahoo.com/v8/finance/chart/";

const BÖRSEN: &[&str] = &[
    "GER", "FRA", /*"STU",*/ "VIE", "PAR", "AMS", "NYQ", "HKG",
];

#[derive(Debug)]
pub struct Metadaten {
    pub symbol: String,
    pub name: String,
}

#[derive(Debug)]
pub struct Kursdaten {
    pub währung: String,
    pub datum: Datum,
    pub open: Zahl,
    pub close: Zahl,
}

#[derive(Debug)]
pub struct Kursabfrage {
    cacher: Cache,
}

impl Kursabfrage {
    pub fn new(cacher: Cache) -> Self {
        Self { cacher }
    }
}

impl Kursabfrage {
    pub async fn aktie_suchen(&self, suche: &str) -> Result<Option<Metadaten>> {
        let key = format!("suche-{suche}");
        let url = format!("{SEARCH_BASE}{suche}");
        let builder = self.cacher.get(&url);
        let list = self.cacher.get_request(&key, builder).await?;
        let list: raw::Search = serde_json::from_str(&list).context("Aktie suchen")?;

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

        let Some((_idx, aktie)) = aktien.into_iter().next() else {
            return Ok(None);
        };

        let name = aktie
            .longname
            .or(aktie.shortname)
            .context("Aktie sollte einen namen haben")?;

        Ok(Some(Metadaten {
            symbol: aktie.symbol,
            name,
        }))
    }

    pub async fn kurse_abrufen(&self, symbol: &str, datum: Datum) -> Result<Vec<Kursdaten>> {
        let vorher = (datum - Days::new(1)).and_hms_opt(0, 0, 0).unwrap();
        let nachher = (datum + Days::new(14)).and_hms_opt(0, 0, 0).unwrap();

        let key = format!("{symbol}-{datum}");
        let url = format!(
            "{CHART_BASE}{symbol}?interval=1d&period1={}&period2={}",
            vorher.timestamp(),
            nachher.timestamp()
        );
        let builder = self.cacher.get(&url);
        let chart = self.cacher.get_request(&key, builder).await?;
        let chart: raw::Chart = serde_json::from_str(&chart).context("Kursdaten abrufen")?;

        let result = chart
            .chart
            .result
            .into_iter()
            .next()
            .context("Kursdaten abfragen sollte ein Ergebnis liefern")?;

        let timestamps = result.timestamp.into_iter();
        let daten = result
            .indicators
            .quote
            .into_iter()
            .next()
            .context("Es sollte ein Chart existieren")?;
        let open = daten.open.into_iter();
        let close = daten.close.into_iter();
        let währung = result.meta.currency;

        let daten: Vec<_> = timestamps
            .zip(open)
            .zip(close)
            .map(|((ts, open), close)| {
                let datum = NaiveDateTime::from_timestamp_opt(ts, 0).unwrap();
                let datum = datum.date();
                Kursdaten {
                    währung: währung.clone(),
                    datum,
                    open: zahl_aus_float(open),
                    close: zahl_aus_float(close),
                }
            })
            .collect();

        Ok(daten)
    }

    pub async fn kurs_abrufen(&self, symbol: &str, datum: Datum) -> Result<Kursdaten> {
        let daten = self.kurse_abrufen(symbol, datum).await?;

        let idx = match daten.binary_search_by_key(&datum, |daten| daten.datum) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };
        Ok(daten.into_iter().nth(idx).unwrap())
    }

    pub async fn kurs_für_isin(&self, isin: &str, datum: Datum) -> Result<Kursdaten> {
        let metadaten = self
            .aktie_suchen(isin)
            .await?
            .with_context(|| format!("Aktie `{isin}` sollte gefunden werden"))?;
        self.kurs_abrufen(&metadaten.symbol, datum).await
    }
}

mod raw {
    use super::*;

    #[derive(Debug, serde::Deserialize)]
    pub struct Search {
        pub quotes: Vec<SearchQuote>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct SearchQuote {
        pub exchange: String,
        pub symbol: String,
        pub shortname: Option<String>,
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
        pub close: Vec<f64>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn aktien_suchen() {
        let cache = Cache::new().await.unwrap();
        let kursabfrage = Kursabfrage::new(cache);
        let siemens = kursabfrage.aktie_suchen("DE0007236101").await.unwrap();
        let siemens_energy = kursabfrage.aktie_suchen("DE000ENER6Y0").await.unwrap();
        dbg!(&siemens, &siemens_energy);

        let datum = Datum::from_ymd_opt(2020, 9, 28).unwrap();

        let kurs = kursabfrage
            .kurs_abrufen(&siemens.unwrap().symbol, datum)
            .await
            .unwrap();
        dbg!(kurs);

        let kurs = kursabfrage
            .kurs_abrufen(&siemens_energy.unwrap().symbol, datum)
            .await
            .unwrap();
        dbg!(kurs);

        let tencent = kursabfrage.aktie_suchen("KYG875721634").await.unwrap();
        let meituan = kursabfrage.aktie_suchen("KYG596691041").await.unwrap();
        dbg!(&tencent, &meituan);

        let datum = Datum::from_ymd_opt(2020, 9, 28).unwrap();

        let kurs = kursabfrage
            .kurs_abrufen(&tencent.unwrap().symbol, datum)
            .await
            .unwrap();
        dbg!(kurs);

        let kurs = kursabfrage
            .kurs_abrufen(&meituan.unwrap().symbol, datum)
            .await
            .unwrap();
        dbg!(kurs);
    }
}
