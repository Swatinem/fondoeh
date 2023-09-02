use std::collections::BTreeMap;

use anyhow::{Context, Result};

const OEKB_LIST_BASE: &str = "https://my.oekb.at/fond-info/rest/public/steuerMeldung/isin";
const OEKB_REPORT_BASE: &str = "https://my.oekb.at/fond-info/rest/public/steuerMeldung/stmId";

// for whatever reason, but its a 500 without this header:
const CONTEXT_HEADER_NAME: &str = "OeKB-Platform-Context";
const CONTEXT_HEADER_VALUE: &str =
    "eyJsYW5ndWFnZSI6ImRlIiwicGxhdGZvcm0iOiJLTVMiLCJkYXNoYm9hcmQiOiJLTVNfT1VUUFVUIn0=";

const ECB_USD: &str = "https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/usd.xml";

mod raw {
    #[derive(Debug, serde::Deserialize)]
    pub struct FondInfo {
        pub list: Vec<FondInfoRow>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct FondInfoRow {
        #[serde(rename = "stmId")]
        pub report_id: usize,
        #[serde(rename = "isinBez")]
        pub name: String,
        #[serde(rename = "zufluss")]
        pub date: String,
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
        pub value: f64,
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
    pub date: String,
    pub report_id: usize,
    pub currency: String,
    pub rate: f64,
    pub yearly_report: String,
    pub StB_E1KV_Ausschuettungen: f64,
    pub StB_E1KV_AGErtraege: f64,
    pub StB_E1KV_anzurechnende_ausl_Quellensteuer: f64,
    pub StB_E1KV_Korrekturbetrag_saldiert: f64,
}

pub async fn fetch_reports(isin: &str) -> Result<Report> {
    let client = reqwest::Client::new();

    let list_url = format!("{OEKB_LIST_BASE}/{isin}");
    // dbg!(&list_url);
    let list = client
        .get(list_url)
        .header(CONTEXT_HEADER_NAME, CONTEXT_HEADER_VALUE)
        .send()
        .await?;

    let list: raw::FondInfo = list.json().await?;

    let mut name = String::new();
    let mut rows = Vec::with_capacity(list.list.len());

    let mut rates: BTreeMap<String, f64> = Default::default();

    for info in list.list {
        // dbg!(&info);

        let (date, _rest) = info
            .date
            .split_once('T')
            .context("should be a ISO8601 datetime")?;

        if date < "2020-01-01" {
            continue;
        }

        let rate = match info.currency.as_str() {
            "EUR" => 1.0,
            "USD" => {
                if rates.is_empty() {
                    rates = fetch_usd_rates().await?;
                    // dbg!(&rates);
                }
                rates.get(date).copied().unwrap_or(1.0)
            }
            currency => anyhow::bail!("currency `{currency}` not supported"),
        };

        let report_url = format!("{OEKB_REPORT_BASE}/{}/privatAnl", info.report_id);
        // dbg!(&report_url);

        let report = client
            .get(report_url)
            .header(CONTEXT_HEADER_NAME, CONTEXT_HEADER_VALUE)
            .send()
            .await?;
        let report: raw::Report = report.json().await?;

        let mut row = ReportRow {
            report_id: info.report_id,
            date: date.into(),
            currency: info.currency,
            rate,
            yearly_report: info.yearly_report,
            ..Default::default()
        };

        for raw_row in report.list {
            // dbg!(&raw_row);

            match raw_row.key.as_str() {
                "StB_E1KV_Ausschuettungen" => row.StB_E1KV_Ausschuettungen = raw_row.value,
                "StB_E1KV_AGErtraege" => row.StB_E1KV_AGErtraege = raw_row.value,
                "StB_E1KV_anzurechnende_ausl_Quellensteuer" => {
                    row.StB_E1KV_anzurechnende_ausl_Quellensteuer = raw_row.value
                }
                "StB_E1KV_Korrekturbetrag_saldiert" => {
                    row.StB_E1KV_Korrekturbetrag_saldiert = raw_row.value
                }
                _ => {}
            }
        }

        name = info.name;
        rows.push(row);
    }

    Ok(Report {
        isin: isin.into(),
        name,
        rows,
    })
}

pub async fn fetch_usd_rates() -> Result<BTreeMap<String, f64>> {
    let doc = reqwest::get(ECB_USD).await?;
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
        let rate: f64 = rate.parse()?;
        rates.insert(date.into(), rate);
    }

    Ok(rates)
}
