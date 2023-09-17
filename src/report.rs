use std::fmt;
use std::fmt::Write;

use num_traits::Zero;

use crate::formatierung::{Eur, Stück};
use crate::{Bestand, Datum, Jahr, Steuer, TransaktionsTyp, Wertpapier, Zahl};

pub const BREITE: usize = 80;

pub struct ReportTitel<'a> {
    pub wertpapier: &'a Wertpapier,
}
impl fmt::Display for ReportTitel<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wertpapier = self.wertpapier;
        writeln!(f, "{} ({})", wertpapier.name, wertpapier.typ)?;
        write!(f, "ISIN: {}", wertpapier.isin)?;
        if let Some(symbol) = &wertpapier.symbol {
            writeln!(f, " ({})", symbol)?;
        } else {
            f.write_char('\n')?;
        }
        writeln!(f, "{:=<BREITE$}", "")
    }
}

pub struct ReportJahr<'a> {
    pub jahr: &'a Jahr,
}
impl fmt::Display for ReportJahr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        schreibe_jahr(f, self.jahr)
    }
}

pub struct ReportBestandAm {
    pub datum: Datum,
    pub bestand: Bestand,
}
impl fmt::Display for ReportBestandAm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut w = Writer::new(f);

        w.write_split_fmt(
            format_args!("{}: Bestand", self.datum),
            ReportBestand(self.bestand.stück, self.bestand.preis),
        )
        // w.divider('=')
    }
}

struct ReportBestand(pub Zahl, pub Zahl);
impl fmt::Display for ReportBestand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Stück", Stück(self.0))?;
        let preis = self.1;
        if !preis.is_zero() {
            write!(f, " × {}", Eur(preis, 4))?;
        }
        Ok(())
    }
}

pub fn schreibe_jahr<W: fmt::Write>(w: &mut W, jahr: &Jahr) -> fmt::Result {
    let mut w = Writer::new(w);

    let bestand = jahr.bestand_anfang;
    w.write_split_fmt(
        format_args!("{}: Bestand", jahr.erster()),
        ReportBestand(bestand.stück, bestand.preis),
    )?;

    for transaktion in &jahr.transaktionen {
        let datum = transaktion.datum;
        let bestand = transaktion.bestand;

        w.divider('-')?;
        match &transaktion.typ {
            TransaktionsTyp::Kauf { stück, preis } => {
                writeln!(w, "{datum}: Kauf {}", ReportBestand(*stück, *preis))?;
            }
            TransaktionsTyp::Verkauf { stück, preis } => {
                writeln!(w, "{datum}: Verkauf {}", ReportBestand(*stück, *preis))?;
            }

            TransaktionsTyp::Split { faktor } => {
                writeln!(w, "{datum}: Aktiensplit mit Faktor {faktor}")?;
            }
            TransaktionsTyp::Ausgliederung { faktor, isin } => {
                writeln!(w, "{datum}: Ausgliederung von `{isin}` mit Faktor {faktor}")?;
            }
            TransaktionsTyp::Einbuchung { stück, preis } => {
                writeln!(
                    w,
                    "{datum}: Einbuchung nach Ausgliederung {}",
                    ReportBestand(*stück, *preis)
                )?;
            }
            TransaktionsTyp::Spitzenverwertung { stück, preis } => {
                writeln!(
                    w,
                    "{datum}: Spitzenverwertung {}",
                    ReportBestand(*stück, *preis)
                )?;
            }

            TransaktionsTyp::Dividende { auszahlung, .. } => {
                writeln!(w, "{datum}: Dividendenzahlung")?;
                writeln!(w, "Auszahlung: {}", Eur(*auszahlung, 2))?;
            }
            TransaktionsTyp::Ausschüttung { brutto, melde_id } => {
                if let Some(melde_id) = melde_id {
                    writeln!(w, "{datum}: Ausschüttung mit Meldung (Id: {melde_id})")?;
                } else {
                    writeln!(w, "{datum}: Ausschüttung ohne Meldung")?;
                }
                writeln!(w, "Auszahlung: {}", Eur(*brutto, 2))?;
            }
            TransaktionsTyp::Jahresmeldung { melde_id } => {
                writeln!(w, "{datum}: Jahresmeldung (Id: {melde_id})")?;
            }
        }
        print_steuern(&mut w, &transaktion.steuer)?;

        if !matches!(
            transaktion.typ,
            TransaktionsTyp::Dividende { .. }
                | TransaktionsTyp::Ausschüttung { melde_id: None, .. }
        ) {
            w.write_split(
                "Neuer Bestand:",
                ReportBestand(bestand.stück, bestand.preis),
            )?;
        }
    }

    w.divider('-')
}

pub struct Writer<W: fmt::Write> {
    inner: W,
    links: String,
    rechts: String,
}

impl<W: fmt::Write> Writer<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            links: String::new(),
            rechts: String::new(),
        }
    }
    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn divider(&mut self, ch: char) -> fmt::Result {
        for _ in 0..BREITE {
            self.inner.write_char(ch)?;
        }
        self.inner.write_char('\n')
    }

    pub fn write_split(&mut self, links: &str, rechts: impl fmt::Display) -> fmt::Result {
        self.write_split_fmt(format_args!("{links}"), rechts)
    }
    fn write_split_fmt(&mut self, links: fmt::Arguments, rechts: impl fmt::Display) -> fmt::Result {
        self.links.clear();
        self.links.write_fmt(links)?;
        self.rechts.clear();
        self.rechts.write_fmt(format_args!("{}", rechts))?;

        let pad = BREITE - 1 - self.links.chars().count();
        writeln!(&mut self.inner, "{} {:>pad$}", self.links, self.rechts)
    }
}

impl<W: fmt::Write> fmt::Write for Writer<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.inner.write_str(s)
    }
}

pub fn print_steuern<W: Write>(w: &mut Writer<W>, steuer: &Steuer) -> fmt::Result {
    match steuer {
        Steuer::Keine => {}
        Steuer::Verkauf(verkauf) => {
            if !verkauf.überschüsse_994.is_zero() {
                w.write_split(
                    "Überschüsse aus realisierten Wertsteigerungen (994):",
                    Eur(verkauf.überschüsse_994, 2),
                )?;
            }
            if !verkauf.verluste_892.is_zero() {
                w.write_split(
                    "Verluste aus realisierten Wertverlusten (892):",
                    Eur(verkauf.verluste_892, 2),
                )?;
            }
        }
        Steuer::Dividende(dividende) => {
            if !dividende.dividendenerträge_863.is_zero() {
                w.write_split(
                    "Einkünfte aus Dividenden (863):",
                    Eur(dividende.dividendenerträge_863, 2),
                )?;
            }
            if !dividende.gezahlte_inländische_kest_899.is_zero() {
                w.write_split(
                    "Gezahlte inländische KeSt (899):",
                    Eur(dividende.gezahlte_inländische_kest_899, 2),
                )?;
            }
            if !dividende.anrechenbare_quellensteuer_998.is_zero() {
                w.write_split(
                    "Anrechenbare ausländische Quellensteuer (998):",
                    Eur(dividende.anrechenbare_quellensteuer_998, 2),
                )?;
            }
        }
        Steuer::Ausschüttung(ausschüttung) => {
            if !ausschüttung.ausschüttungen_898.is_zero() {
                w.write_split(
                    "Ausschüttungen (898):",
                    Eur(ausschüttung.ausschüttungen_898, 2),
                )?;
            }
            if !ausschüttung.ausschüttungsgleiche_erträge_937.is_zero() {
                w.write_split(
                    "Ausschüttungsgleiche Erträge (937):",
                    Eur(ausschüttung.ausschüttungsgleiche_erträge_937, 2),
                )?;
            }
            if !ausschüttung.anrechenbare_quellensteuer_998.is_zero() {
                w.write_split(
                    "Anrechenbare ausländische Quellensteuer (998):",
                    Eur(ausschüttung.anrechenbare_quellensteuer_998, 2),
                )?;
            }
        }
    }
    Ok(())
}
