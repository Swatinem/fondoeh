use std::borrow::Cow;

use anyhow::{Context, Result};

use crate::cacher::Cacher;
use crate::format;
use crate::waehrungen::{Kurs, Währungen};
use crate::{Datum, String, Zahl};

const OEKB_LIST_BASE: &str = "https://my.oekb.at/fond-info/rest/public/steuerMeldung/isin";
const OEKB_REPORT_BASE: &str = "https://my.oekb.at/fond-info/rest/public/steuerMeldung/stmId";

// for whatever reason, but its a 500 without this header:
const CONTEXT_HEADER_NAME: &str = "OeKB-Platform-Context";
const CONTEXT_HEADER_VALUE: &str =
    "eyJsYW5ndWFnZSI6ImRlIiwicGxhdGZvcm0iOiJLTVMiLCJkYXNoYm9hcmQiOiJLTVNfT1VUUFVUIn0=";

mod raw {
    use super::*;

    use serde_json::value::RawValue;

    #[derive(Debug, serde::Deserialize)]
    pub struct FondMeldungen {
        pub list: Vec<FondMeldung>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct FondMeldung {
        #[serde(rename = "stmId")]
        pub melde_id: u32,
        #[serde(rename = "isinBez")]
        pub name: String,
        #[serde(rename = "zufluss")]
        pub zufluss: String,
        #[serde(rename = "zuflussFmv")] // für korrigierende Meldungen
        pub zufluss_korrigiert: Option<String>,
        #[serde(rename = "gueltBis")] // für korrigierte Meldungen
        pub gültig_bis: Option<String>,
        #[serde(rename = "waehrung")]
        pub währung: String,
        #[serde(rename = "jahresdatenmeldung")]
        pub ist_jahresmeldung: String,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct Meldungsdetails {
        pub list: Vec<Meldungsdetail>,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct Meldungsdetail {
        #[serde(rename = "steuerName")]
        pub key: String,
        #[serde(rename = "pvMitOption4")]
        pub value: RawZahl,
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(try_from = "&RawValue")]
    pub struct RawZahl(pub Zahl);

    impl TryFrom<&RawValue> for RawZahl {
        type Error = anyhow::Error;

        fn try_from(value: &RawValue) -> Result<Self, Self::Error> {
            let raw = Cow::Borrowed(value.get());
            let zahl = format::Zahl::try_from(raw)?;
            Ok(Self(zahl.0))
        }
    }
}

#[derive(Debug)]
pub struct FondMeldungen {
    pub isin: String,
    pub name: String,
    pub meldungen: Vec<FondMeldung>,
}

#[allow(non_snake_case)]
#[derive(Debug, Default)]
pub struct FondMeldung {
    pub datum: Datum,
    pub melde_id: u32,
    pub ist_jahresmeldung: bool,
    pub währung: String,
    pub währungskurs: Zahl,
    pub StB_E1KV_Ausschuettungen: Zahl,
    pub StB_E1KV_AGErtraege: Zahl,
    pub StB_E1KV_anzurechnende_ausl_Quellensteuer: Zahl,
    pub StB_E1KV_Korrekturbetrag_saldiert: Zahl,
}

#[derive(Debug)]
pub struct Meldungen {
    heute: Datum,
    cacher: Cacher,
    währungen: Währungen,
}

impl Meldungen {
    pub fn new(cacher: Cacher, währungen: Währungen) -> Self {
        let heute = chrono::Local::now().date_naive();
        Self {
            heute,
            cacher,
            währungen,
        }
    }

    pub async fn fetch_meldungen(&self, isin: &str) -> Result<FondMeldungen> {
        let key = format!("meldungen-{isin}-{}", self.heute);
        let url = format!("{OEKB_LIST_BASE}/{isin}");
        let builder = self
            .cacher
            .get(&url)
            .header(CONTEXT_HEADER_NAME, CONTEXT_HEADER_VALUE);
        let list = self.cacher.get_request(&key, builder).await?;
        let list: raw::FondMeldungen = serde_json::from_str(&list).context("Meldungen einlesen")?;

        let mut name = String::new("");
        let mut meldungen = Vec::with_capacity(list.list.len());

        for info in list.list {
            // Meldung wurde von einer anderen Meldung korrigiert?
            if info.gültig_bis.is_some() {
                continue;
            }
            let datum = info.zufluss_korrigiert.unwrap_or(info.zufluss);
            let (datum, _rest) = datum.split_once('T').unwrap_or((&datum, ""));
            let datum = datum.parse()?;

            let row = FondMeldung {
                melde_id: info.melde_id,
                datum,
                währung: info.währung,
                ist_jahresmeldung: info.ist_jahresmeldung == "JA",
                ..Default::default()
            };
            name = info.name;
            meldungen.push(row);
        }

        meldungen.sort_by_key(|r| r.datum);

        Ok(FondMeldungen {
            isin: isin.into(),
            name,
            meldungen,
        })
    }

    pub async fn fetch_meldungs_details(&mut self, meldung: &mut FondMeldung) -> Result<()> {
        let key = format!("meldung-{}-privatAnl", meldung.melde_id);
        let url = format!("{OEKB_REPORT_BASE}/{}/privatAnl", meldung.melde_id);
        let builder = self
            .cacher
            .get(&url)
            .header(CONTEXT_HEADER_NAME, CONTEXT_HEADER_VALUE);
        let raw_details = self.cacher.get_request(&key, builder).await?;
        let raw_details: raw::Meldungsdetails =
            serde_json::from_str(&raw_details).context("Meldungsdetails einlesen")?;

        meldung.währungskurs = self
            .währungen
            .kurs_in_euro(Kurs {
                wert: 1.into(),
                währung: meldung.währung.clone(),
                datum: meldung.datum,
            })
            .await?;

        for raw_row in raw_details.list {
            match raw_row.key.as_str() {
                "StB_E1KV_Ausschuettungen" => meldung.StB_E1KV_Ausschuettungen = raw_row.value.0,
                "StB_E1KV_AGErtraege" => meldung.StB_E1KV_AGErtraege = raw_row.value.0,
                "StB_E1KV_anzurechnende_ausl_Quellensteuer" => {
                    meldung.StB_E1KV_anzurechnende_ausl_Quellensteuer = raw_row.value.0
                }
                "StB_E1KV_Korrekturbetrag_saldiert" => {
                    meldung.StB_E1KV_Korrekturbetrag_saldiert = raw_row.value.0
                }
                _ => {}
            }
        }

        // TODO: fetch AIF?

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_korrigierter_report() {
        let cacher = Cacher::new().await.unwrap();
        let währungen = Währungen::new(cacher.clone());
        let mut meldungen = Meldungen::new(cacher, währungen);

        let mut report = meldungen.fetch_meldungen("IE00B9CQXS71").await.unwrap();
        dbg!(&report);

        let meldung = report.meldungen.last_mut().unwrap();
        meldungen.fetch_meldungs_details(meldung).await.unwrap();
        dbg!(meldung);
    }
}
