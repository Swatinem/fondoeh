use num_traits::identities::Zero;

use crate::meldungen::FondMeldung;
use crate::{Bestand, TransaktionsTyp, Zahl};
use crate::{Steuer, SteuerAusschüttung, SteuerDividende, SteuerVerkauf};

type Ergebnis = (Bestand, TransaktionsTyp, Steuer);

// Laut § 27a (4) 3. gilt:
// [B]ei Erwerb in zeitlicher Aufeinanderfolge [ist] der gleitende
// Durchschnittspreis […] anzusetzen.
pub fn kauf_berechnen(bestand: Bestand, stück: Zahl, preis: Zahl) -> Ergebnis {
    let stück_neu = bestand.stück + stück;
    let preis_neu = runde((bestand.summe() + (stück * preis)) / stück_neu, 4);

    (
        Bestand {
            stück: stück_neu,
            preis: preis_neu,
        },
        TransaktionsTyp::Kauf { stück, preis },
        Steuer::Keine,
    )
}

// Das gleiche wie ein Kauf, nur wollen wir einen anderen Typ für die Ausgabe
pub fn einbuchung_berechnen(bestand: Bestand, stück: Zahl, preis: Zahl) -> Ergebnis {
    let (bestand, _transaktion, steuer) = kauf_berechnen(bestand, stück, preis);
    (
        bestand,
        TransaktionsTyp::Einbuchung { stück, preis },
        steuer,
    )
}

// Laut § 27a (3) 2. gilt:
// Als Einkünfte anzusetzen sind
// [b]ei realisierten Wertsteigerungen […] der Unterschiedsbetrag
// zwischen dem Veräußerungserlös […] und den Anschaffungskosten.
pub fn verkauf_berechnen(mut bestand: Bestand, stück: Zahl, preis: Zahl) -> Ergebnis {
    let einstand = stück * bestand.preis;
    let erlös = stück * preis;

    bestand.stück -= stück;
    if bestand.stück.is_zero() {
        bestand.preis = 0.into();
    }

    let mut steuer = SteuerVerkauf::default();
    if erlös > einstand {
        steuer.überschüsse_994 = erlös - einstand;
    } else {
        steuer.verluste_892 = einstand - erlös;
    }

    (
        bestand,
        TransaktionsTyp::Verkauf { stück, preis },
        Steuer::Verkauf(steuer),
    )
}

// Das gleiche wie ein Verkauf, nur wollen wir einen anderen Typ für die Ausgabe
pub fn spitzenverwertung_berechnen(bestand: Bestand, stück: Zahl, preis: Zahl) -> Ergebnis {
    let (bestand, _transaktion, steuer) = verkauf_berechnen(bestand, stück, preis);
    (
        bestand,
        TransaktionsTyp::Spitzenverwertung { stück, preis },
        steuer,
    )
}

// Laut § 6 (1) gilt:
// Wird im Zuge eines Aktiensplits der Nennwert einer Aktie heruntergesetzt und daher
// die Anzahl der ausgegebenen Aktien erhöht, sind die bisherigen Anschaffungskosten auf die
// im Zuge des Aktiensplits ausgegebenen Aktien aufzuteilen.
pub fn split_berechnen(mut bestand: Bestand, faktor: Zahl) -> Ergebnis {
    bestand.stück *= faktor;
    bestand.preis = runde(bestand.preis / faktor, 4);

    (bestand, TransaktionsTyp::Split { faktor }, Steuer::Keine)
}

pub fn dividende_berechnen(
    bestand: Bestand,
    isin: &str,
    brutto: Zahl,
    auszahlung: Zahl,
) -> Ergebnis {
    let mut steuer = SteuerDividende {
        dividendenerträge_863: brutto,
        ..Default::default()
    };

    let gezahlte_quellensteuer = brutto - auszahlung;
    if isin.starts_with("AT") {
        // sind die quellensteuern für AT aktien im ausland jetzt 899 oder 998?
        steuer.gezahlte_inländische_kest_899 = gezahlte_quellensteuer;
    } else {
        // TODO: hängt dies vom Land ab?
        let anrechenbarer_quellensteuersatz = Zahl::new(15, 100);
        steuer.anrechenbare_quellensteuer_998 = runde(
            (brutto * anrechenbarer_quellensteuersatz).min(gezahlte_quellensteuer),
            2,
        );
    }

    (
        bestand,
        TransaktionsTyp::Dividende { brutto, auszahlung },
        Steuer::Dividende(steuer),
    )
}

pub fn ausschüttung_berechnen(
    mut bestand: Bestand,
    auszahlung: Zahl,
    meldung: Option<FondMeldung>,
) -> Ergebnis {
    let mut steuer = SteuerAusschüttung::default();

    let Some(meldung) = meldung else {
        steuer.ausschüttungen_898 = auszahlung;
        return (
            bestand,
            TransaktionsTyp::Ausschüttung {
                brutto: auszahlung,
                melde_id: None,
            },
            Steuer::Ausschüttung(steuer),
        );
    };

    let melde_id = Some(meldung.melde_id);
    let steuer = meldung_berechnen(&mut bestand, &meldung);
    (
        bestand,
        TransaktionsTyp::Ausschüttung {
            brutto: auszahlung,
            melde_id,
        },
        steuer,
    )
}

pub fn meldung_berechnen(bestand: &mut Bestand, meldung: &FondMeldung) -> Steuer {
    let prostück = bestand.stück / meldung.währungskurs;

    let steuer = SteuerAusschüttung {
        ausschüttungen_898: runde(meldung.StB_E1KV_Ausschuettungen * prostück, 2),
        ausschüttungsgleiche_erträge_937: runde(meldung.StB_E1KV_AGErtraege * prostück, 2),
        anrechenbare_quellensteuer_998: runde(
            meldung.StB_E1KV_anzurechnende_ausl_Quellensteuer * prostück,
            2,
        ),
    };

    let korrektur = runde(
        meldung.StB_E1KV_Korrekturbetrag_saldiert / meldung.währungskurs,
        4,
    );
    bestand.preis += korrektur;

    Steuer::Ausschüttung(steuer)
}

fn runde(zahl: Zahl, stellen: u32) -> Zahl {
    let faktor = 10_i64.pow(stellen);
    (zahl * faktor).round() / faktor
}
