use std::collections::BTreeMap;

use anyhow::Result;

use crate::{Datum, Zahl};

const OEKB_LIST_BASE: &str = "https://my.oekb.at/fond-info/rest/public/steuerMeldung/isin";
const OEKB_REPORT_BASE: &str = "https://my.oekb.at/fond-info/rest/public/steuerMeldung/stmId";

// for whatever reason, but its a 500 without this header:
const CONTEXT_HEADER_NAME: &str = "OeKB-Platform-Context";
const CONTEXT_HEADER_VALUE: &str =
    "eyJsYW5ndWFnZSI6ImRlIiwicGxhdGZvcm0iOiJLTVMiLCJkYXNoYm9hcmQiOiJLTVNfT1VUUFVUIn0=";

const ECB_USD: &str = "https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/usd.xml";

mod raw {
    use super::*;
    use crate::format;

    #[derive(Debug, serde::Deserialize)]
    pub struct FondInfo {
        pub list: Vec<FondInfoRow>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct FondInfoRow {
        #[serde(rename = "stmId")]
        pub report_id: u32,
        #[serde(rename = "isinBez")]
        pub name: String,
        #[serde(rename = "zufluss")]
        pub zufluss: Datum,
        #[serde(rename = "zuflussFmv")] // für korrigierende Meldungen
        pub zufluss_korrigiert: Option<Datum>,
        #[serde(rename = "gueltBis")] // für korrigierte Meldungen
        pub gültig_bis: Option<Datum>,
        #[serde(rename = "waehrung")]
        pub currency: String,
        #[serde(rename = "jahresdatenmeldung")]
        pub yearly_report: String,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct Report {
        pub list: Vec<ReportRow>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct ReportRow {
        #[serde(rename = "steuerName")]
        pub key: String,
        #[serde(rename = "pvMitOption4")]
        pub value: format::Zahl,
    }
}

#[derive(Debug)]
pub struct Report {
    pub isin: String,
    pub name: String,
    pub rows: Vec<ReportRow>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default)]
pub struct ReportRow {
    pub date: Datum,
    pub report_id: u32,
    pub currency: String,
    pub is_yearly_report: bool,
    pub rate: Zahl,
    pub StB_E1KV_Ausschuettungen: Zahl,
    pub StB_E1KV_AGErtraege: Zahl,
    pub StB_E1KV_anzurechnende_ausl_Quellensteuer: Zahl,
    pub StB_E1KV_Korrekturbetrag_saldiert: Zahl,
}

#[derive(Debug)]
pub struct Scraper {
    client: reqwest::Client,
    usd_rates: BTreeMap<Datum, Zahl>,
}

impl Scraper {
    pub fn new() -> Self {
        let client = reqwest::Client::new();
        let usd_rates = Default::default();

        Self { client, usd_rates }
    }

    async fn get_currency_rate(&mut self, currency: &str, date: Datum) -> Zahl {
        match currency {
            "EUR" => return 1.into(),
            "USD" => {}
            _ => {
                panic!("Currency {currency} not supported")
            }
        }
        if self.usd_rates.is_empty() {
            self.usd_rates = self.fetch_usd_rates().await.unwrap();
        }
        self.usd_rates.get(&date).copied().unwrap_or(1.into())
    }

    async fn fetch_usd_rates(&self) -> Result<BTreeMap<Datum, Zahl>> {
        let doc = self.client.get(ECB_USD).send().await?;
        let doc = doc.text().await?;

        let mut rates = BTreeMap::new();

        for line in doc.lines() {
            // <Obs TIME_PERIOD="2023-09-01" OBS_VALUE="1.0844" OBS_STATUS="A" OBS_CONF="F"/>
            let Some(line) = line.trim_start().strip_prefix("<Obs TIME_PERIOD=\"") else {
                continue;
            };
            let Some((date, rest)) = line.split_once("\" OBS_VALUE=\"") else {
                continue;
            };
            let Some((rate, _rest)) = rest.split_once("\" OBS") else {
                continue;
            };
            let date: Datum = date.parse()?;
            let rate: Zahl = rate.parse()?;
            rates.insert(date, rate);
        }

        Ok(rates)
    }

    pub async fn fetch_reports(&self, isin: &str) -> Result<Report> {
        let list_url = format!("{OEKB_LIST_BASE}/{isin}");
        let list = self
            .client
            .get(list_url)
            .header(CONTEXT_HEADER_NAME, CONTEXT_HEADER_VALUE)
            .send()
            .await?;

        let list: raw::FondInfo = list.json().await?;

        let mut name = String::new();
        let mut rows = Vec::with_capacity(list.list.len());

        for info in list.list {
            // Meldung wurde von einer anderen Meldung korrigiert?
            if info.gültig_bis.is_some() {
                continue;
            }
            let date = info.zufluss_korrigiert.unwrap_or(info.zufluss);

            let row = ReportRow {
                report_id: info.report_id,
                date,
                currency: info.currency,
                is_yearly_report: info.yearly_report == "JA",
                ..Default::default()
            };
            name = info.name;
            rows.push(row);
        }

        rows.sort_by_key(|r| r.date);

        Ok(Report {
            isin: isin.into(),
            name,
            rows,
        })
    }

    pub async fn fetch_report_details(&mut self, report: &mut ReportRow) -> Result<()> {
        let details_url = format!("{OEKB_REPORT_BASE}/{}/privatAnl", report.report_id);
        let raw_details = self
            .client
            .get(details_url)
            .header(CONTEXT_HEADER_NAME, CONTEXT_HEADER_VALUE)
            .send()
            .await?;
        let raw_details: raw::Report = raw_details.json().await?;

        report.rate = self.get_currency_rate(&report.currency, report.date).await;

        for raw_row in raw_details.list {
            match raw_row.key.as_str() {
                "StB_E1KV_Ausschuettungen" => report.StB_E1KV_Ausschuettungen = raw_row.value.0,
                "StB_E1KV_AGErtraege" => report.StB_E1KV_AGErtraege = raw_row.value.0,
                "StB_E1KV_anzurechnende_ausl_Quellensteuer" => {
                    report.StB_E1KV_anzurechnende_ausl_Quellensteuer = raw_row.value.0
                }
                "StB_E1KV_Korrekturbetrag_saldiert" => {
                    report.StB_E1KV_Korrekturbetrag_saldiert = raw_row.value.0
                }
                _ => {}
            }
        }

        // TODO: fetch AIF?

        Ok(())
    }
}

impl Default for Scraper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_korrigierter_report() {
        let scraper = Scraper::new();

        let report = scraper.fetch_reports("IE00B9CQXS71").await.unwrap();
        dbg!(report);
    }
}
