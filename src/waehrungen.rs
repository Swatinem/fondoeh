use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};

use crate::cache::Cache;
use crate::{Datum, String, Zahl};

const ECB_BASE: &str =
    "https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html";

#[derive(Debug)]
pub struct Kurs {
    pub wert: Zahl,
    pub währung: String,
    pub datum: Datum,
}

#[derive(Debug)]
pub struct Währungen {
    heute: Datum,
    cacher: Cache,
    cache: HashMap<String, BTreeMap<Datum, Zahl>>,
}

impl Währungen {
    pub fn new(cacher: Cache) -> Self {
        let heute = chrono::Local::now().date_naive();
        Self {
            heute,
            cacher,
            cache: Default::default(),
        }
    }

    async fn kurse_abrufen(&self, währung: &str) -> Result<BTreeMap<Datum, Zahl>> {
        let key = format!("{währung}-{}", self.heute);
        let url = format!("{ECB_BASE}/{}.xml", währung.to_lowercase());
        let builder = self.cacher.get(&url);
        let doc = self.cacher.get_request(&key, builder).await?;

        let mut kurse = BTreeMap::new();

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
            let rate: Zahl = crate::format::Zahl::try_from(Cow::Borrowed(rate))?.0;
            kurse.insert(date, rate);
        }

        Ok(kurse)
    }

    #[tracing::instrument(skip(self))]
    pub async fn kurs_in_euro(&mut self, kurs: Kurs) -> Result<Zahl> {
        if kurs.währung == "EUR" {
            return Ok(kurs.wert);
        }
        if !self.cache.contains_key(&kurs.währung) {
            let kurse = self.kurse_abrufen(&kurs.währung).await?;
            self.cache.insert(kurs.währung.clone(), kurse);
        }
        let kurse = self
            .cache
            .get(&kurs.währung)
            .expect("Kurse sollten verfügbar sein");

        let umrechnungskurs = kurse
            .get(&kurs.datum)
            .copied()
            .context("Es sollte einen Umrechnungskurs geben")?;

        Ok(kurs.wert / umrechnungskurs)
    }
}
