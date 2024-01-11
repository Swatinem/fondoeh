pub use crate::format::{Datum, Rational64 as Zahl, String, WertpapierTyp};

#[derive(Debug)]
pub struct Wertpapier {
    pub typ: WertpapierTyp,
    pub name: String,
    pub isin: String,
    pub symbol: Option<String>,
    pub jahre: Vec<Jahr>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Bestand {
    pub stück: Zahl,
    pub preis: Zahl,
}

#[derive(Debug, Default)]
pub struct Jahr {
    pub jahr: i32,
    pub bestand_anfang: Bestand,
    pub bestand_ende: Bestand,
    pub transaktionen: Vec<Transaktion>,
}

#[derive(Debug)]
pub struct Transaktion {
    pub datum: Datum,
    pub bestand: Bestand,
    pub typ: TransaktionsTyp,
    pub steuer: Steuer,
}

#[derive(Debug)]

pub enum TransaktionsTyp {
    Kauf { stück: Zahl, preis: Zahl },
    Verkauf { stück: Zahl, preis: Zahl },

    Split { faktor: Zahl },
    Ausgliederung { faktor: Zahl, isin: String },
    Einbuchung { stück: Zahl, preis: Zahl },
    Spitzenverwertung { stück: Zahl, preis: Zahl },

    Dividende { brutto: Zahl, auszahlung: Zahl },
    Ausschüttung { brutto: Zahl, melde_id: Option<u32> },
    Jahresmeldung { melde_id: u32 },
}

#[derive(Debug, Clone, Copy)]
pub enum Steuer {
    Keine,
    Verkauf(SteuerVerkauf),
    Dividende(SteuerDividende),
    Ausschüttung(SteuerAusschüttung),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SteuerVerkauf {
    pub überschüsse_994: Zahl,
    pub verluste_892: Zahl,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SteuerDividende {
    pub dividendenerträge_863: Zahl,
    pub gezahlte_inländische_kest_899: Zahl,
    pub anrechenbare_quellensteuer_998: Zahl,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SteuerAusschüttung {
    pub ausschüttungen_898: Zahl,
    pub ausschüttungsgleiche_erträge_937: Zahl,
    pub anrechenbare_quellensteuer_998: Zahl,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SteuerJahr {
    pub jahr: i32,

    pub überschüsse_994: Zahl,
    pub verluste_892: Zahl,

    pub dividendenerträge_863: Zahl,

    pub ausschüttungen_898: Zahl,
    pub ausschüttungsgleiche_erträge_937: Zahl,

    pub gezahlte_inländische_kest_899: Zahl,
    pub anrechenbare_quellensteuer_998: Zahl,
}

impl SteuerJahr {
    pub fn new(jahr: i32) -> Self {
        Self {
            jahr,
            ..Default::default()
        }
    }
}

pub fn zahl_aus_float(f: f64) -> Zahl {
    let zahl = (f * 10_000.).round() as i64;
    Zahl::new(zahl, 10_000)
}

impl Bestand {
    pub fn summe(&self) -> Zahl {
        self.stück * self.preis
    }
}

impl Wertpapier {
    pub fn iter_jahre(&self, jahr: Option<i32>) -> impl Iterator<Item = &Jahr> {
        self.jahre
            .iter()
            .filter(move |j| jahr.is_none() || Some(j.jahr) == jahr)
    }
}

impl Jahr {
    pub fn erster(&self) -> Datum {
        Datum::from_ymd_opt(self.jahr, 1, 1).unwrap()
    }
    pub fn letzter(&self) -> Datum {
        Datum::from_ymd_opt(self.jahr, 12, 31).unwrap()
    }
}
