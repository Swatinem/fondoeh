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
    Kauf {
        stück: Zahl,
        preis: Zahl,
    },
    Verkauf {
        stück: Zahl,
        preis: Zahl,
    },
    Split {
        faktor: Zahl,
    },
    Dividende {
        brutto: Zahl,
        auszahlung: Zahl,
    },
    Ausschüttung {
        brutto: Zahl,
        melde_id: Option<NonZeroU32>,
    },
    Jahresmeldung {
        melde_id: NonZeroU32,
    },
}

#[derive(Debug)]
pub enum Steuer {
    Keine,
}

impl Wertpapier {
    pub fn jahr(&self, jahr: i32) -> Option<&Jahr> {
        self.jahre.iter().find(|j| j.jahr == jahr)
    }
}
