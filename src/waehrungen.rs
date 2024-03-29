use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::Mutex;

use crate::cacher::Cacher;
use crate::{Datum, String, Zahl};

const ECB_BASE: &str =
    "https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html";

#[derive(Debug)]
pub struct Kurs {
    pub wert: Zahl,
    pub währung: String,
    pub datum: Datum,
}

#[derive(Debug, Clone)]
pub struct Währungen {
    inner: Arc<Mutex<WährungenInner>>,
}

#[derive(Debug)]
struct WährungenInner {
    heute: Datum,
    cacher: Cacher,
    cache: HashMap<String, BTreeMap<Datum, Zahl>>,
}

impl Währungen {
    pub fn new(cacher: Cacher) -> Self {
        let heute = chrono::Local::now().date_naive();
        let inner = Arc::new(Mutex::new(WährungenInner {
            heute,
            cacher,
            cache: Default::default(),
        }));
        Self { inner }
    }

    #[tracing::instrument(err, skip(self))]
    pub async fn kurs_in_euro(&mut self, kurs: Kurs) -> Result<Zahl> {
        if kurs.währung == "EUR" {
            return Ok(kurs.wert);
        }

        let mut inner = self.inner.lock().await;
        inner.kurs_in_euro(kurs).await
    }
}

impl WährungenInner {
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

    async fn kurs_in_euro(&mut self, kurs: Kurs) -> Result<Zahl> {
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
