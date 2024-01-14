use std::ops::AddAssign;

use num_traits::identities::Zero;

use crate::meldungen::FondMeldung;
use crate::{Bestand, SteuerJahr, String, TransaktionsTyp, Zahl};
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

// Laut § 4 (2) gilt:
// Werden Aktien im Zuge einer Abspaltung auf ein Wertpapierdepot eingebucht,
// sind für Zwecke des Kapitalertragsteuerabzuges die Anschaffungskosten der vor
// der Abspaltung bestehenden Aktien auf die bestehenden und die neu eingebuchten
// Aktien aufzuteilen. Diese Aufteilung hat im Verhältnis der Verkehrswerte der
// bestehenden zu den eingebuchten Aktien zu erfolgen.
pub fn ausgliederung_berechnen(
    mut bestand: Bestand,
    faktor: Zahl,
    isin: String,
    eigener_kurs: Zahl,
    anderer_kurs: Zahl,
) -> Ergebnis {
    let eigener_bestand = Bestand {
        stück: bestand.stück,
        preis: eigener_kurs,
    };
    let anderer_bestand = Bestand {
        stück: bestand.stück * faktor,
        preis: anderer_kurs,
    };

    let gesamtwert = eigener_bestand.summe() + anderer_bestand.summe();
    let prozentual = eigener_bestand.summe() / gesamtwert;

    bestand.preis = runde(bestand.preis * prozentual, 4);

    (
        bestand,
        TransaktionsTyp::Ausgliederung { faktor, isin },
        Steuer::Keine,
    )
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

// Laut § 186 (2) 3. InvFG 2011:
// ausschüttungsgleichen Erträge […], sind diese in Höhe von 90 vH des
// Unterschiedsbetrages zwischen dem ersten und letzten im Kalenderjahr
// festgesetzten Rücknahmepreis, mindestens jedoch in Höhe von 10 vH des
// am Ende des Kalenderjahres festgesetzten Rücknahmepreises zu schätzen.
pub fn ausschüttungsgleiche_beträge_berechnen(
    stück: Zahl,
    wert_anfang: Zahl,
    wert_ende: Zahl,
) -> (Zahl, TransaktionsTyp, Steuer) {
    let mut steuer = SteuerAusschüttung::default();

    dbg!(wert_anfang, wert_ende);
    let unterschied = wert_ende - wert_anfang;
    steuer.ausschüttungsgleiche_erträge_937 =
        (unterschied * Zahl::new(90, 100)).max(wert_ende * Zahl::new(10, 100));

    // Erhöhung der Anschaffungskosten:
    let korrektur = runde(steuer.ausschüttungsgleiche_erträge_937 / stück, 4);

    (
        korrektur,
        TransaktionsTyp::Ausschüttung {
            brutto: Zahl::default(),
            melde_id: None,
        },
        Steuer::Ausschüttung(steuer),
    )
}

fn runde(zahl: Zahl, stellen: u32) -> Zahl {
    let faktor = 10_i64.pow(stellen);
    (zahl * faktor).round() / faktor
}

impl SteuerJahr {
    pub fn berechne_nachzahlung(&self) -> Zahl {
        let zu_versteuern = self.überschüsse_994 - self.verluste_892
            + self.dividendenerträge_863
            + self.ausschüttungen_898
            + self.ausschüttungsgleiche_erträge_937;
        let steuer = zu_versteuern * Zahl::new(275, 1000);
        steuer - self.gezahlte_inländische_kest_899 - self.anrechenbare_quellensteuer_998
    }
}

impl AddAssign<Steuer> for SteuerJahr {
    fn add_assign(&mut self, steuer: Steuer) {
        match steuer {
            Steuer::Keine => {}
            Steuer::Verkauf(s) => {
                self.überschüsse_994 += s.überschüsse_994;
                self.verluste_892 += s.verluste_892;
            }
            Steuer::Dividende(s) => {
                self.dividendenerträge_863 += s.dividendenerträge_863;
                self.gezahlte_inländische_kest_899 += s.gezahlte_inländische_kest_899;
                self.anrechenbare_quellensteuer_998 += s.anrechenbare_quellensteuer_998;
            }
            Steuer::Ausschüttung(s) => {
                self.ausschüttungen_898 += s.ausschüttungen_898;
                self.ausschüttungsgleiche_erträge_937 += s.ausschüttungsgleiche_erträge_937;
                self.anrechenbare_quellensteuer_998 += s.anrechenbare_quellensteuer_998;
            }
        }
    }
}
