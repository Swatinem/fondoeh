use std::fmt;

use num_traits::Zero;

use crate::{Bestand, Datum, Jahr, TransaktionsTyp, Wertpapier};
use crate::{Steuer, SteuerAusschüttung, SteuerDividende, SteuerVerkauf};

pub struct TsvTitel;
impl fmt::Display for TsvTitel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        schreibe_titel(f)
    }
}

pub struct TsvWertpapier<'a> {
    pub wertpapier: &'a Wertpapier,
    pub jahr: &'a Jahr,
}
impl fmt::Display for TsvWertpapier<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        schreibe_tsv(f, self.wertpapier, self.jahr)
    }
}

pub fn schreibe_titel<W: fmt::Write>(w: &mut W) -> fmt::Result {
    write!(w, "Name\tISIN\tArt\tDatum\tBestand\tDurchschnittspreis\t")?; // 6
    write!(w, "Aktion\tStück\tPreis\tBrutto\tAuszahlung\tMelde-ID\t")?; // 6
    write!(w, "Überschuss (994)\tVerlust (892)\t")?; // 2
    write!(
        w,
        "Dividendenertrag (863)\tGezahlte KeSt (899)\tAnrechenbare Quellensteuer (998)\t"
    )?; // 3
    writeln!(
        w,
        "Ausschüttung (898)\tAusschüttungsgl Ertrag (937)\tAnrechenbare Quellensteuer (998)"
    ) // 3
}

pub fn schreibe_tsv<W: fmt::Write>(
    w: &mut W,
    wertpapier: &Wertpapier,
    daten: &Jahr,
) -> fmt::Result {
    schreibe_anfang(w, wertpapier, daten.erster(), daten.bestand_anfang)?;
    writeln!(w, "Bestand")?;

    for transaktion in &daten.transaktionen {
        schreibe_anfang(w, wertpapier, transaktion.datum, transaktion.bestand)?;

        match transaktion.typ {
            TransaktionsTyp::Kauf { stück, preis } => {
                write!(w, "Kauf\t{}\t{}\t\t\t\t", stück, preis)?;
            }
            TransaktionsTyp::Verkauf { stück, preis } => {
                write!(w, "Verkauf\t{}\t{}\t\t\t\t", stück, preis)?;
            }
            TransaktionsTyp::Split { faktor } => {
                write!(w, "Split\t{}\t\t\t\t\t", faktor)?;
            }
            TransaktionsTyp::Dividende { brutto, auszahlung } => {
                write!(w, "Dividende\t\t\t{}\t{}\t\t", brutto, auszahlung)?;
            }
            TransaktionsTyp::Ausschüttung { brutto, melde_id } => {
                let aktion = if melde_id.is_some() {
                    "Ausschüttung mit Meldung"
                } else {
                    "Ausschüttung ohne Meldung"
                };
                write!(w, "{}\t\t\t{}\t\t", aktion, brutto)?;
                if let Some(melde_id) = melde_id {
                    write!(w, "{melde_id}")?;
                }
                w.write_char('\t')?;
            }
            TransaktionsTyp::Jahresmeldung { melde_id } => {
                write!(w, "Jahresmeldung\t\t\t\t\t{}\t", melde_id)?;
            }
        }

        schreibe_steuern(w, transaktion.steuer)?;

        w.write_char('\n')?;
    }

    schreibe_anfang(w, wertpapier, daten.letzter(), daten.bestand_ende)?;
    writeln!(w, "Bestand")
}

fn schreibe_anfang<W: fmt::Write>(
    w: &mut W,
    wertpapier: &Wertpapier,
    datum: Datum,
    bestand: Bestand,
) -> fmt::Result {
    write!(
        w,
        "{}\t{}\t{:?}\t{}\t{}\t{}\t",
        wertpapier.name, wertpapier.isin, wertpapier.typ, datum, bestand.stück, bestand.preis,
    )
}

fn schreibe_steuern<W: fmt::Write>(w: &mut W, steuer: Steuer) -> fmt::Result {
    let mut verkauf = SteuerVerkauf::default();
    let mut dividende = SteuerDividende::default();
    let mut ausschüttung = SteuerAusschüttung::default();
    match steuer {
        Steuer::Keine => {}
        Steuer::Verkauf(steuer) => verkauf = steuer,
        Steuer::Dividende(steuer) => dividende = steuer,
        Steuer::Ausschüttung(steuer) => ausschüttung = steuer,
    }

    if !verkauf.überschüsse_994.is_zero() {
        write!(w, "{}", verkauf.überschüsse_994)?;
    }
    w.write_char('\t')?;
    if !verkauf.verluste_892.is_zero() {
        write!(w, "{}", verkauf.verluste_892)?;
    }
    w.write_char('\t')?;

    if !dividende.dividendenerträge_863.is_zero() {
        write!(w, "{}", dividende.dividendenerträge_863)?;
    }
    w.write_char('\t')?;
    if !dividende.gezahlte_inländische_kest_899.is_zero() {
        write!(w, "{}", dividende.gezahlte_inländische_kest_899)?;
    }
    w.write_char('\t')?;
    if !dividende.anrechenbare_quellensteuer_998.is_zero() {
        write!(w, "{}", dividende.anrechenbare_quellensteuer_998)?;
    }
    w.write_char('\t')?;

    if !ausschüttung.ausschüttungen_898.is_zero() {
        write!(w, "{}", ausschüttung.ausschüttungen_898)?;
    }
    w.write_char('\t')?;
    if !ausschüttung.ausschüttungsgleiche_erträge_937.is_zero() {
        write!(w, "{}", ausschüttung.ausschüttungsgleiche_erträge_937)?;
    }
    w.write_char('\t')?;
    if !ausschüttung.anrechenbare_quellensteuer_998.is_zero() {
        write!(w, "{}", ausschüttung.anrechenbare_quellensteuer_998)?;
    }
    w.write_char('\t')
}
