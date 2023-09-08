use std::num::NonZeroU32;

pub use crate::format::{Datum, Rational64 as Zahl, WertpapierTyp};

#[derive(Debug)]
pub struct Wertpapier {
    pub typ: WertpapierTyp,
    pub name: String,
    pub isin: String,
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
    Dividende { brutto: Zahl, auszahlung: Zahl },
    Ausschüttung { brutto: Zahl, melde_id: Option<u32> },
    Jahresmeldung { melde_id: u32 },
}

#[derive(Debug)]
pub enum Steuer {
    Keine,
    Verkauf(SteuerVerkauf),
    Dividende(SteuerDividende),
    Ausschüttung(SteuerAusschüttung),
}

#[derive(Debug, Default)]
pub struct SteuerVerkauf {
    pub überschüsse_994: Zahl,
    pub verluste_892: Zahl,
}

#[derive(Debug, Default)]
pub struct SteuerDividende {
    pub dividendenerträge_863: Zahl,
    pub gezahlte_inländische_kest_899: Zahl,
    pub anrechenbare_quellensteuer_998: Zahl,
}

#[derive(Debug, Default)]
pub struct SteuerAusschüttung {
    pub ausschüttungen_898: Zahl,
    pub ausschüttungsgleiche_erträge_937: Zahl,
    pub anrechenbare_quellensteuer_998: Zahl,
}

impl Bestand {
    pub fn summe(&self) -> Zahl {
        self.stück * self.preis
    }
}

impl Wertpapier {
    pub fn jahr(&self, jahr: i32) -> Option<&Jahr> {
        self.jahre.iter().find(|j| j.jahr == jahr)
    }
}
