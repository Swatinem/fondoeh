use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use globset::GlobBuilder;
use walkdir::WalkDir;

use berechnung::Rechner;
use fondoeh::*;
use report::BREITE;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Pfad zu den Daten
    daten: Vec<PathBuf>,

    /// TSV Ausgabe aktivieren
    #[arg(short, long)]
    tsv: bool,

    /// Für welches Jahr die Berechnung erfolgen soll
    #[arg(short, long)]
    jahr: Option<i32>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let gefundene_daten = finde_alle_daten(args.daten)?;

    let mut rechner = Rechner::new();

    let mut wertpapiere = Vec::with_capacity(gefundene_daten.len());
    for (pfad, wertpapier) in gefundene_daten {
        let context = format!("Auswertung von `{}` ({})", wertpapier.name, pfad.display());
        let wertpapier = rechner
            .wertpapier_auswerten(wertpapier)
            .await
            .context(context)?;
        wertpapiere.push(wertpapier);
    }

    wertpapiere.sort_by(|a, b| (&a.typ, &a.name).cmp(&(&b.typ, &b.name)));
    // dbg!(&wertpapiere);

    let mut w = std::io::stdout().lock();
    if args.tsv {
        write!(w, "{}", tsv::TsvTitel)?;
    }

    let mut wertpapiere = wertpapiere.iter().peekable();

    while let Some(wertpapier) = wertpapiere.next() {
        let mut jahre = wertpapier.iter_jahre(args.jahr).peekable();
        if jahre.peek().is_none() {
            continue;
        };

        if !args.tsv {
            write!(w, "{}", report::ReportTitel { wertpapier })?;
        }

        let mut letztes_jahr = None;
        for jahr in jahre {
            if args.tsv {
                write!(w, "{}", tsv::TsvWertpapier { wertpapier, jahr })?;
            } else {
                write!(w, "{}", report::ReportJahr { jahr })?;
            }
            letztes_jahr = Some(jahr);
        }
        if !args.tsv {
            let letztes_jahr = letztes_jahr.unwrap();
            let datum = letztes_jahr.letzter().min(rechner.heute);
            let bestand = report::ReportBestandAm {
                datum,
                bestand: letztes_jahr.bestand_ende,
            };
            write!(w, "{bestand}")?;

            if wertpapiere.peek().is_some() {
                writeln!(w)?;
                writeln!(w, "{:#<BREITE$}", "")?;
                writeln!(w)?;
            }
        }
    }

    Ok(())
}

fn finde_alle_daten(daten: Vec<PathBuf>) -> Result<HashMap<PathBuf, format::Wertpapier>> {
    let mut gefundene_daten = HashMap::new();

    let glob = GlobBuilder::new("**/*.{yml,yaml}")
        .case_insensitive(true)
        .build()?
        .compile_matcher();

    for pfad in daten {
        for entry in WalkDir::new(pfad) {
            let entry = entry?;
            let pfad = entry.path();
            if glob.is_match(pfad) {
                if gefundene_daten.contains_key(pfad) {
                    continue;
                }
                let rdr = fs::File::open(pfad)
                    .with_context(|| format!("Öffnen von `{}`", pfad.display()))?;
                let wertpapier = serde_yaml::from_reader(rdr)
                    .with_context(|| format!("Einlesen von `{}`", pfad.display()))?;

                gefundene_daten.insert(entry.into_path(), wertpapier);
            }
        }
    }

    Ok(gefundene_daten)
}
