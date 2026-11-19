//! Libreria di conversione XPS -> PDF.
//!
//! Pipeline:
//! 1. apertura del pacchetto XPS (archivio ZIP) e lettura di FixedDocumentSequence;
//! 2. parsing di ogni FixedPage (percorsi, immagini, glifi con testo Unicode);
//! 3. scrittura del PDF con font incorporati e testo selezionabile.

mod font;
mod geom;
mod pdf;
mod render;
mod xpspkg;

use std::fmt;
use std::fs;
use std::io;

use render::Renderer;
use xpspkg::{parse_dom, risolvi_uri, Node, Package};

#[derive(Debug)]
pub enum ConvertError {
    Io(io::Error),
    /// Il contenuto XPS non è valido o non ancora supportato.
    Unsupported(String),
    /// Il pacchetto o il markup XML non sono leggibili.
    Malformed(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Io(e) => write!(f, "I/O: {e}"),
            ConvertError::Unsupported(m) => write!(f, "non supportato: {m}"),
            ConvertError::Malformed(m) => write!(f, "file non valido: {m}"),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<io::Error> for ConvertError {
    fn from(e: io::Error) -> Self {
        ConvertError::Io(e)
    }
}

/// Converte il file XPS `input` nel PDF `output`.
pub fn convert_file(input: &str, output: &str) -> Result<(), ConvertError> {
    let xps = fs::read(input)?;
    let pdf = convert_bytes(&xps)?;
    if let Some(cartella) = std::path::Path::new(output).parent() {
        if !cartella.as_os_str().is_empty() {
            fs::create_dir_all(cartella)?;
        }
    }
    fs::write(output, pdf)?;
    Ok(())
}

/// Converte il file XPS `input` in un PDF «poster»: ogni pagina viene
/// distribuita su `colonne` × `righe` fogli dello stesso formato.
pub fn convert_file_poster(
    input: &str,
    output: &str,
    colonne: u32,
    righe: u32,
) -> Result<(), ConvertError> {
    let xps = fs::read(input)?;
    let pdf = convert_bytes_poster(&xps, colonne, righe)?;
    if let Some(cartella) = std::path::Path::new(output).parent() {
        if !cartella.as_os_str().is_empty() {
            fs::create_dir_all(cartella)?;
        }
    }
    fs::write(output, pdf)?;
    Ok(())
}

/// Converte i byte di un pacchetto XPS in byte di un PDF.
pub fn convert_bytes(xps: &[u8]) -> Result<Vec<u8>, ConvertError> {
    convert_bytes_poster(xps, 1, 1)
}

/// Come `convert_bytes`, con la distribuzione «poster» su più fogli.
pub fn convert_bytes_poster(xps: &[u8], colonne: u32, righe: u32) -> Result<Vec<u8>, ConvertError> {
    let pacchetto = Package::open(xps)?;
    let pagine = raccogli_pagine(&pacchetto)?;
    if pagine.is_empty() {
        return Err(ConvertError::Unsupported(
            "nessuna pagina trovata nel documento XPS".into(),
        ));
    }

    let mut renderer = Renderer::new(&pacchetto);
    let mut convertite = Vec::with_capacity(pagine.len());
    for (nodo, parte) in &pagine {
        convertite.push(renderer.render_page(nodo, parte)?);
    }
    let convertite = if colonne > 1 || righe > 1 {
        espandi_poster(convertite, colonne.max(1), righe.max(1))
    } else {
        convertite
    };
    Ok(renderer.finish(convertite))
}

/// Divide ogni pagina in `colonne` × `righe` pagine di formato uguale
/// all'originale: il contenuto viene ingrandito e ogni foglio ne mostra
/// una porzione, da incollare insieme come un poster.
fn espandi_poster(
    pagine: Vec<render::PaginaPdf>,
    colonne: u32,
    righe: u32,
) -> Vec<render::PaginaPdf> {
    let mut risultato = Vec::with_capacity(pagine.len() * (colonne * righe) as usize);
    for pagina in pagine {
        let (w, h) = (pagina.larghezza, pagina.altezza);
        for riga in 0..righe {
            for colonna in 0..colonne {
                let x = -(colonna as f64) * w;
                let y = -(riga as f64) * h;
                let contenuto = format!(
                    "q\n0 0 {w:.3} {h:.3} re W n\n{colonne} 0 0 {righe} {x:.3} {y:.3} cm\n{}\nQ\n",
                    pagina.contenuto
                );
                risultato.push(render::PaginaPdf {
                    larghezza: w,
                    altezza: h,
                    contenuto,
                    font: pagina.font.clone(),
                    immagini: pagina.immagini.clone(),
                });
            }
        }
    }
    risultato
}

/// Individua le pagine del documento seguendo FixedDocumentSequence e
/// FixedDocument; se i riferimenti mancano ripiega sulle pagine presenti.
/// I tipi sono riconosciuti dall'elemento radice, così funzionano anche i
/// pacchetti che usano estensioni `.xaml` o parti interleaved.
fn raccogli_pagine(pacchetto: &Package) -> Result<Vec<(Node, String)>, ConvertError> {
    let mut sequenze: Vec<(String, Node)> = Vec::new();
    let mut documenti: Vec<(String, Node)> = Vec::new();
    let mut pagine_sciolte: Vec<(String, Node)> = Vec::new();

    for nome in pacchetto.part_names() {
        if !sembra_markup(nome) {
            continue;
        }
        let Some(dati) = pacchetto.part_bytes(nome) else {
            continue;
        };
        let Ok(dom) = parse_dom(&dati) else {
            continue;
        };
        match dom.name.as_str() {
            "FixedDocumentSequence" => sequenze.push((nome.clone(), dom)),
            "FixedDocument" => documenti.push((nome.clone(), dom)),
            "FixedPage" => pagine_sciolte.push((nome.clone(), dom)),
            _ => {}
        }
    }

    let mut riferimenti: Vec<String> = Vec::new();
    for (nome, dom) in &sequenze {
        for nodo in dom.descendants() {
            if nodo.name.ends_with("DocumentReference") {
                if let Some(sorgente) = nodo.attr("Source") {
                    riferimenti.push(risolvi_uri(nome, sorgente));
                }
            }
        }
    }

    let mut documenti_scelti: Vec<(String, Node)> = Vec::new();
    for riferimento in &riferimenti {
        let Some(dati) = pacchetto.part_bytes(riferimento) else {
            continue;
        };
        let Ok(dom) = parse_dom(&dati) else {
            continue;
        };
        if dom.name == "FixedDocument" {
            documenti_scelti.push((riferimento.clone(), dom));
        }
    }
    if documenti_scelti.is_empty() {
        documenti.sort_by_key(|a| chiave_naturale(&a.0));
        documenti_scelti = documenti;
    }

    let mut pagine: Vec<(Node, String)> = Vec::new();
    for (nome, dom) in &documenti_scelti {
        for nodo in dom.descendants() {
            if nodo.name.ends_with("PageContent") {
                if let Some(sorgente) = nodo.attr("Source") {
                    let parte = risolvi_uri(nome, sorgente);
                    let Some(dati) = pacchetto.part_bytes(&parte) else {
                        continue;
                    };
                    let Ok(dom) = parse_dom(&dati) else {
                        continue;
                    };
                    pagine.push((dom, parte));
                }
            }
        }
    }

    if pagine.is_empty() {
        pagine_sciolte.sort_by_key(|a| chiave_naturale(&a.0));
        pagine = pagine_sciolte
            .into_iter()
            .map(|(nome, dom)| (dom, nome))
            .collect();
    }

    Ok(pagine)
}

fn sembra_markup(nome: &str) -> bool {
    matches!(
        nome.rsplit('.').next(),
        Some("xaml" | "fdseq" | "fdoc" | "fpage")
    )
}

/// Ordinamento naturale: `2.fpage` viene prima di `10.fpage`.
fn chiave_naturale(nome: &str) -> (u64, String) {
    let ultimo = nome.rsplit('/').next().unwrap_or(nome);
    let cifre: String = ultimo.chars().take_while(|c| c.is_ascii_digit()).collect();
    (cifre.parse().unwrap_or(0), nome.to_string())
}
