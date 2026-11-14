//! Conversione di una pagina XPS (`FixedPage`) in contenuto PDF.
//!
//! Le coordinate XPS (1/96 di pollice, asse Y verso il basso) vengono portate
//! nei punti tipografici del PDF con una matrice applicata all'inizio del
//! flusso; il testo usa font CID con codifica Identity-H così i glifi restano
//! vettoriali e il testo estratto dal PDF resta selezionabile e ricercabile.

use crate::font::FontInfo;
use crate::geom::{numeri, parse_geometry, Mat, Segment};
use crate::pdf::Pdf;
use crate::xpspkg::{risolvi_uri, Node, Package};
use crate::ConvertError;
use std::collections::BTreeMap;

/// Pagina già convertita in un flusso di contenuto PDF.
pub struct PaginaPdf {
    pub larghezza: f64,
    pub altezza: f64,
    pub contenuto: String,
    pub font: BTreeMap<String, u32>,
    pub immagini: BTreeMap<String, u32>,
}

struct FontEntry {
    info: FontInfo,
    nome_pdf: String,
    tipo0: u32,
    discendente: u32,
    to_unicode: u32,
    descrittore: u32,
    usati: BTreeMap<u16, Option<char>>,
    larghezze: BTreeMap<u16, u32>,
}

#[derive(Clone)]
enum Pennello {
    Nessuno,
    Tinta([f64; 3]),
    Immagine {
        parte: String,
        area: Option<(f64, f64, f64, f64)>,
    },
    /// Riempimento con un albero di disegno (`VisualBrush`), tipico delle pagine
    /// generate dalla stampante virtuale di Windows.
    Visuale {
        visuale: Node,
        mappatura: Mat,
    },
}

struct Contesto<'a> {
    risorse: &'a BTreeMap<String, Node>,
    base: &'a str,
}

pub struct Renderer<'a> {
    pkg: &'a Package,
    pdf: Pdf,
    font: BTreeMap<String, FontEntry>,
    immagini: BTreeMap<String, (String, u32)>,
    profondita_visuali: usize,
}

impl<'a> Renderer<'a> {
    pub fn new(pkg: &'a Package) -> Self {
        Renderer {
            pkg,
            pdf: Pdf::new(),
            font: BTreeMap::new(),
            immagini: BTreeMap::new(),
            profondita_visuali: 0,
        }
    }

    pub fn render_page(&mut self, pagina: &Node, parte: &str) -> Result<PaginaPdf, ConvertError> {
        let larghezza_xps = attr_f64(pagina, "Width").unwrap_or(816.0);
        let altezza_xps = attr_f64(pagina, "Height").unwrap_or(1056.0);
        let mut risultato = PaginaPdf {
            larghezza: larghezza_xps * 0.75,
            altezza: altezza_xps * 0.75,
            contenuto: String::new(),
            font: BTreeMap::new(),
            immagini: BTreeMap::new(),
        };
        risultato
            .contenuto
            .push_str(&format!("0.75 0 0 -0.75 0 {:.4} cm\n", altezza_xps * 0.75));

        let risorse = raccogli_risorse(pagina);
        let contesto = Contesto {
            risorse: &risorse,
            base: parte,
        };
        let base = transform_of(pagina);
        for figlio in &pagina.children {
            self.walk(figlio, base, &contesto, &mut risultato)?;
        }
        Ok(risultato)
    }

    fn walk(
        &mut self,
        nodo: &Node,
        padre: Mat,
        ctx: &Contesto<'_>,
        pagina: &mut PaginaPdf,
    ) -> Result<(), ConvertError> {
        // Le risorse sono definizioni (pennelli, font, alberi visuali): non si
        // disegnano direttamente, vanno usate tramite riferimento.
        if nodo.name.ends_with(".Resources") || nodo.name == "ResourceDictionary" {
            return Ok(());
        }
        let matrice = padre.mul(transform_of(nodo));
        match nodo.name.as_str() {
            "Path" => self.draw_path(nodo, matrice, ctx, pagina),
            "Glyphs" => self.draw_glyphs(nodo, matrice, ctx, pagina),
            _ => {
                for figlio in &nodo.children {
                    self.walk(figlio, matrice, ctx, pagina)?;
                }
                Ok(())
            }
        }
    }

    fn draw_path(
        &mut self,
        nodo: &Node,
        matrice: Mat,
        ctx: &Contesto<'_>,
        pagina: &mut PaginaPdf,
    ) -> Result<(), ConvertError> {
        let Some(data) = nodo.attr("Data") else {
            return Ok(());
        };
        let geometria = parse_geometry(data);
        if geometria.is_empty() {
            return Ok(());
        }
        let riempimento = self.pennello(nodo, "Fill", ctx);
        let contorno = self.pennello(nodo, "Stroke", ctx);
        let spessore = attr_f64(nodo, "StrokeThickness").unwrap_or(1.0);

        let mut percorso = String::new();
        for segmento in &geometria.segments {
            match *segmento {
                Segment::Move(p) => percorso.push_str(&format!("{:.3} {:.3} m\n", p.0, p.1)),
                Segment::Line(p) => percorso.push_str(&format!("{:.3} {:.3} l\n", p.0, p.1)),
                Segment::Curve(a, b, c) => percorso.push_str(&format!(
                    "{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n",
                    a.0, a.1, b.0, b.1, c.0, c.1
                )),
                Segment::Close => percorso.push_str("h\n"),
            }
        }

        let regola = if geometria.even_odd { "f*" } else { "f" };
        let regola_combinata = if geometria.even_odd { "B*" } else { "B" };
        let regola_ritaglio = if geometria.even_odd { "W* n" } else { "W n" };

        let mut ops = String::from("q\n");
        ops.push_str(&matrice.operands());
        ops.push_str(" cm\n");

        let mut disegnato = false;
        match riempimento.clone() {
            Pennello::Tinta(c) => {
                ops.push_str(&format!("{:.4} {:.4} {:.4} rg\n", c[0], c[1], c[2]));
                if let Pennello::Tinta(s) = contorno.clone() {
                    ops.push_str(&format!(
                        "{:.4} {:.4} {:.4} RG\n{:.3} w\n",
                        s[0], s[1], s[2], spessore
                    ));
                    ops.push_str(&percorso);
                    ops.push_str(regola_combinata);
                    ops.push('\n');
                } else {
                    ops.push_str(&percorso);
                    ops.push_str(regola);
                    ops.push('\n');
                }
                disegnato = true;
            }
            Pennello::Immagine { .. } | Pennello::Visuale { .. } => {
                ops.push_str(&percorso);
                ops.push_str(regola_ritaglio);
                ops.push('\n');
            }
            Pennello::Nessuno => {
                if let Pennello::Tinta(s) = contorno.clone() {
                    ops.push_str(&format!(
                        "{:.4} {:.4} {:.4} RG\n{:.3} w\n",
                        s[0], s[1], s[2], spessore
                    ));
                    ops.push_str(&percorso);
                    ops.push_str("S\n");
                    disegnato = true;
                }
            }
        }

        if let Pennello::Immagine { parte, area } = riempimento.clone() {
            let parte = risolvi_uri(ctx.base, &parte);
            let riquadro = area
                .or_else(|| geometria.bounds())
                .unwrap_or((0.0, 0.0, 1.0, 1.0));
            let larghezza = riquadro.2 - riquadro.0;
            let altezza = riquadro.3 - riquadro.1;
            if larghezza > 0.0 && altezza > 0.0 {
                if let Some((nome, id)) = self.registra_immagine(&parte) {
                    ops.push_str(&format!(
                        "q\n{:.4} 0 0 {:.4} {:.4} {:.4} cm\n/{nome} Do\nQ\n",
                        larghezza, -altezza, riquadro.0, riquadro.3
                    ));
                    pagina.immagini.insert(nome, id);
                    disegnato = true;
                }
            }
        }

        if let Pennello::Visuale { visuale, mappatura } = riempimento {
            pagina.contenuto.push_str(&ops);
            pagina
                .contenuto
                .push_str(&format!("{} cm\n", mappatura.operands()));
            self.profondita_visuali += 1;
            if self.profondita_visuali <= 4 {
                for figlio in &visuale.children {
                    self.walk(figlio, Mat::IDENTITY, ctx, pagina)?;
                }
            }
            self.profondita_visuali -= 1;
            pagina.contenuto.push_str("Q\n");
            return Ok(());
        }

        ops.push_str("Q\n");
        if disegnato {
            pagina.contenuto.push_str(&ops);
        }
        Ok(())
    }

    fn draw_glyphs(
        &mut self,
        nodo: &Node,
        matrice: Mat,
        ctx: &Contesto<'_>,
        pagina: &mut PaginaPdf,
    ) -> Result<(), ConvertError> {
        let (Some(font_uri), Some(em)) =
            (nodo.attr("FontUri"), attr_f64(nodo, "FontRenderingEmSize"))
        else {
            return Ok(());
        };
        if em <= 0.0 {
            return Ok(());
        }
        let origine_x = attr_f64(nodo, "OriginX").unwrap_or(0.0);
        let origine_y = attr_f64(nodo, "OriginY").unwrap_or(0.0);
        let colore_testo = match self.pennello(nodo, "Fill", ctx) {
            Pennello::Tinta(c) => c,
            _ => [0.0, 0.0, 0.0],
        };
        let parte = risolvi_uri(ctx.base, font_uri);
        if !self.registra_font(&parte)? {
            return Ok(());
        }

        // `Indices` elenca, per ogni carattere di `UnicodeString`, l'indice del
        // glifo e/o la sua larghezza di avanzamento. Va preferito alla cmap:
        // nei font sottoinsieme della stampante virtuale la cmap può mancare.
        let indici = nodo.attr("Indices").map(parse_indices).unwrap_or_default();
        let caratteri: Vec<char> = nodo
            .attr("UnicodeString")
            .unwrap_or("")
            .chars()
            .filter(|c| *c != '\r' && *c != '\n')
            .collect();

        let mut glifi: Vec<Glifo> = Vec::new();
        if let Some(entry) = self.font.get(&parte) {
            let info = &entry.info;
            if !caratteri.is_empty() {
                for (posizione, carattere) in caratteri.iter().enumerate() {
                    let indice = indici.get(posizione);
                    let gid = indice
                        .and_then(|i| i.gid)
                        .or_else(|| info.glyph_index(*carattere));
                    let Some(gid) = gid else {
                        continue;
                    };
                    let avanzamento = match indice.and_then(|i| i.avanzamento) {
                        Some(percento) => em * percento / 100.0,
                        None => info.advance(gid) * em / info.units_per_em,
                    };
                    glifi.push(Glifo {
                        gid,
                        avanzamento,
                        carattere: Some(*carattere),
                    });
                }
            } else {
                for indice in &indici {
                    let Some(gid) = indice.gid else {
                        continue;
                    };
                    let avanzamento = match indice.avanzamento {
                        Some(percento) => em * percento / 100.0,
                        None => info.advance(gid) * em / info.units_per_em,
                    };
                    glifi.push(Glifo {
                        gid,
                        avanzamento,
                        carattere: None,
                    });
                }
            }
        }
        if glifi.is_empty() {
            return Ok(());
        }

        let (nome_font, tipo0) = {
            let entry = self
                .font
                .get_mut(&parte)
                .ok_or_else(|| ConvertError::Unsupported(format!("font non caricato: {parte}")))?;
            for glifo in &glifi {
                let larghezza = entry.info.width_1000(glifo.gid);
                entry.larghezze.entry(glifo.gid).or_insert(larghezza);
                let voce = entry.usati.entry(glifo.gid).or_insert(None);
                if voce.is_none() {
                    *voce = glifo.carattere;
                }
            }
            (entry.nome_pdf.clone(), entry.tipo0)
        };

        let mut ops = String::from("q\n");
        ops.push_str(&matrice.operands());
        ops.push_str(" cm\nBT\n");
        ops.push_str(&format!("/{nome_font} {em:.3} Tf\n"));
        ops.push_str(&format!(
            "{:.4} {:.4} {:.4} rg\n",
            colore_testo[0], colore_testo[1], colore_testo[2]
        ));
        let mut cursore = 0.0;
        for glifo in &glifi {
            let gid = glifo.gid;
            ops.push_str(&format!(
                "1 0 0 -1 {:.3} {:.3} Tm\n<{gid:04X}> Tj\n",
                origine_x + cursore,
                origine_y
            ));
            cursore += glifo.avanzamento;
        }
        ops.push_str("ET\nQ\n");
        pagina.contenuto.push_str(&ops);
        pagina.font.insert(nome_font, tipo0);
        Ok(())
    }

    fn pennello(&self, nodo: &Node, proprieta: &str, ctx: &Contesto<'_>) -> Pennello {
        if let Some(valore) = nodo.attr(proprieta) {
            let pennello = self.pennello_da_valore(valore, ctx);
            if !matches!(pennello, Pennello::Nessuno) {
                return pennello;
            }
        }
        let suffisso = format!(".{proprieta}");
        if let Some(contenitore) = nodo
            .children
            .iter()
            .find(|figlio| figlio.name.ends_with(&suffisso))
        {
            for figlio in &contenitore.children {
                let pennello = self.pennello_da_nodo(figlio, ctx);
                if !matches!(pennello, Pennello::Nessuno) {
                    return pennello;
                }
            }
        }
        Pennello::Nessuno
    }

    fn pennello_da_valore(&self, valore: &str, ctx: &Contesto<'_>) -> Pennello {
        let valore = valore.trim();
        if valore.is_empty() {
            return Pennello::Nessuno;
        }
        if let Some(chiave) = chiave_risorsa(valore) {
            return match ctx.risorse.get(&chiave) {
                Some(nodo) => self.pennello_da_nodo(nodo, ctx),
                None => Pennello::Nessuno,
            };
        }
        match colore(valore) {
            Some(tinta) => Pennello::Tinta(tinta),
            None => Pennello::Nessuno,
        }
    }

    fn pennello_da_nodo(&self, nodo: &Node, ctx: &Contesto<'_>) -> Pennello {
        match nodo.name.as_str() {
            "SolidColorBrush" => nodo
                .attr("Color")
                .and_then(colore)
                .map(Pennello::Tinta)
                .unwrap_or(Pennello::Nessuno),
            "ImageBrush" => {
                let Some(sorgente) = nodo.attr("ImageSource") else {
                    return Pennello::Nessuno;
                };
                let unita_assolute = nodo
                    .attr("ViewportUnits")
                    .map(|u| u.eq_ignore_ascii_case("Absolute"))
                    .unwrap_or(false);
                let area = if unita_assolute {
                    nodo.attr("Viewport").and_then(rettangolo)
                } else {
                    None
                };
                Pennello::Immagine {
                    parte: risolvi_uri(ctx.base, sorgente),
                    area,
                }
            }
            "VisualBrush" => {
                let Some(visuale) = nodo
                    .child("VisualBrush.Visual")
                    .and_then(|contenitore| contenitore.children.first())
                else {
                    return Pennello::Nessuno;
                };
                Pennello::Visuale {
                    visuale: visuale.clone(),
                    mappatura: mappatura_visualbrush(nodo),
                }
            }
            _ => Pennello::Nessuno,
        }
    }

    fn registra_font(&mut self, parte: &str) -> Result<bool, ConvertError> {
        if self.font.contains_key(parte) {
            return Ok(true);
        }
        let Some(grezzo) = self.pkg.part(parte) else {
            return Ok(false);
        };
        let Some(info) = FontInfo::load(grezzo, parte) else {
            return Ok(false);
        };
        let mut nome_pdf = info.postscript_name.clone();
        if nome_pdf.is_empty() {
            nome_pdf = "StampaCPFont".to_string();
        }
        if self.font.values().any(|f| f.nome_pdf == nome_pdf) {
            nome_pdf.push_str(&format!("{}", self.font.len() + 2));
        }
        let font_file = self
            .pdf
            .add_stream(&format!(" /Length1 {}", info.data.len()), &info.data);
        let descrittore = self.pdf.add(
            format!(
                "<< /Type /FontDescriptor /FontName /{nome_pdf} /Flags {} /FontBBox [{:.1} {:.1} {:.1} {:.1}] /ItalicAngle {:.1} /Ascent {:.1} /Descent {:.1} /CapHeight {:.1} /StemV 80 /FontFile2 {font_file} 0 R >>",
                info.flags,
                info.bbox[0],
                info.bbox[1],
                info.bbox[2],
                info.bbox[3],
                info.italic_angle,
                info.ascent,
                info.descent,
                info.cap_height
            )
            .into_bytes(),
        );
        let discendente = self.pdf.alloc();
        let to_unicode = self.pdf.alloc();
        let tipo0 = self.pdf.add(
            format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{nome_pdf} /Encoding /Identity-H /DescendantFonts [{discendente} 0 R] /ToUnicode {to_unicode} 0 R >>"
            )
            .into_bytes(),
        );
        self.font.insert(
            parte.to_string(),
            FontEntry {
                info,
                nome_pdf,
                tipo0,
                discendente,
                to_unicode,
                descrittore,
                usati: BTreeMap::new(),
                larghezze: BTreeMap::new(),
            },
        );
        Ok(true)
    }

    fn registra_immagine(&mut self, parte: &str) -> Option<(String, u32)> {
        if let Some(voce) = self.immagini.get(parte) {
            return Some(voce.clone());
        }
        let dati = self.pkg.part(parte)?;
        let immagine = image::load_from_memory(dati).ok()?;
        let rgb = immagine.to_rgb8();
        let (larghezza, altezza) = rgb.dimensions();
        if larghezza == 0 || altezza == 0 {
            return None;
        }
        let id = self.pdf.add_stream(
            &format!(
                " /Type /XObject /Subtype /Image /Width {larghezza} /Height {altezza} /ColorSpace /DeviceRGB /BitsPerComponent 8"
            ),
            rgb.as_raw(),
        );
        let nome = format!("Im{}", self.immagini.len() + 1);
        let voce = (nome, id);
        self.immagini.insert(parte.to_string(), voce.clone());
        Some(voce)
    }

    /// Chiude i font, impagina le pagine e restituisce il PDF completo.
    pub fn finish(mut self, pagine: Vec<PaginaPdf>) -> Vec<u8> {
        struct FontFinale {
            discendente: u32,
            descrittore: u32,
            to_unicode: u32,
            nome_pdf: String,
            larghezze: BTreeMap<u16, u32>,
            usati: BTreeMap<u16, Option<char>>,
        }
        let finali: Vec<FontFinale> = self
            .font
            .values()
            .map(|entry| FontFinale {
                discendente: entry.discendente,
                descrittore: entry.descrittore,
                to_unicode: entry.to_unicode,
                nome_pdf: entry.nome_pdf.clone(),
                larghezze: entry.larghezze.clone(),
                usati: entry.usati.clone(),
            })
            .collect();

        for font in finali {
            let dettaglio = format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {} 0 R /CIDToGIDMap /Identity /DW 1000 {} >>",
                font.nome_pdf,
                font.descrittore,
                array_larghezze(&font.larghezze)
            );
            self.pdf.put(font.discendente, dettaglio.into_bytes());
            let cmap = cmap_unicode(&font.usati);
            self.pdf
                .put(font.to_unicode, Pdf::stream_body("", cmap.as_bytes()));
        }

        let albero_pagine = self.pdf.alloc();
        let mut figli = Vec::new();
        for pagina in &pagine {
            let contenuto = self.pdf.add_stream("", pagina.contenuto.as_bytes());
            let mut risorse_font = String::new();
            for (nome, id) in &pagina.font {
                risorse_font.push_str(&format!("/{nome} {id} 0 R "));
            }
            let mut risorse_immagini = String::new();
            for (nome, id) in &pagina.immagini {
                risorse_immagini.push_str(&format!("/{nome} {id} 0 R "));
            }
            let mut risorse = String::from("<< /ProcSet [/PDF /Text /ImageB /ImageC /ImageI]");
            if !risorse_font.is_empty() {
                risorse.push_str(&format!(" /Font << {risorse_font} >>"));
            }
            if !risorse_immagini.is_empty() {
                risorse.push_str(&format!(" /XObject << {risorse_immagini} >>"));
            }
            risorse.push_str(" >>");

            let id = self.pdf.add(
                format!(
                    "<< /Type /Page /Parent {albero_pagine} 0 R /MediaBox [0 0 {:.3} {:.3}] /Resources {risorse} /Contents {contenuto} 0 R >>",
                    pagina.larghezza, pagina.altezza
                )
                .into_bytes(),
            );
            figli.push(format!("{id} 0 R"));
        }

        self.pdf.put(
            albero_pagine,
            format!(
                "<< /Type /Pages /Kids [{}] /Count {} >>",
                figli.join(" "),
                figli.len()
            )
            .into_bytes(),
        );
        let catalogo = self
            .pdf
            .add(format!("<< /Type /Catalog /Pages {albero_pagine} 0 R >>").into_bytes());
        let info = self
            .pdf
            .add(b"<< /Producer (StampaCP) /Creator (StampaCP) >>".to_vec());
        self.pdf.finish(catalogo, Some(info))
    }
}

/// Voce della proprietà `Indices` di un elemento `Glyphs`.
struct Indice {
    gid: Option<u16>,
    /// Larghezza di avanzamento in centesimi di em.
    avanzamento: Option<f64>,
}

struct Glifo {
    gid: u16,
    avanzamento: f64,
    carattere: Option<char>,
}

/// Interpreta `Indices`: voci separate da `;`, ognuna `glifo,avanzamento`
/// (entrambi i valori possono essere omessi).
fn parse_indices(testo: &str) -> Vec<Indice> {
    let mut risultato: Vec<Indice> = Vec::new();
    let voci: Vec<&str> = if testo.contains(';') {
        testo.split(';').collect()
    } else {
        testo.split_whitespace().collect()
    };
    for voce in voci {
        let voce = voce.trim();
        let mut parti = voce.splitn(2, ',');
        let parte_gid = parti.next().unwrap_or("").trim();
        let parte_avanzamento = parti.next().map(|p| p.trim()).unwrap_or("");
        let gid = parte_gid
            .parse::<u32>()
            .ok()
            .filter(|valore| *valore <= u16::MAX as u32)
            .map(|valore| valore as u16);
        let avanzamento = parte_avanzamento
            .parse::<f64>()
            .ok()
            .filter(|valore| valore.is_finite());
        risultato.push(Indice { gid, avanzamento });
    }
    // L'eventuale `;` finale non rappresenta un carattere.
    while matches!(
        risultato.last(),
        Some(Indice {
            gid: None,
            avanzamento: None
        })
    ) {
        risultato.pop();
    }
    risultato
}

fn attr_f64(nodo: &Node, nome: &str) -> Option<f64> {
    nodo.attr(nome)?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn raccogli_risorse(radice: &Node) -> BTreeMap<String, Node> {
    let mut risorse = BTreeMap::new();
    for nodo in radice.descendants() {
        if let Some(chiave) = nodo.attr_locale("Key") {
            risorse
                .entry(chiave.to_string())
                .or_insert_with(|| nodo.clone());
        }
    }
    risorse
}

fn transform_of(nodo: &Node) -> Mat {
    let mut matrice = Mat::IDENTITY;
    for chiave in ["RenderTransform", "Transform"] {
        if let Some(valore) = nodo.attr(chiave) {
            if let Some(t) = Mat::from_xps(valore) {
                matrice = matrice.mul(t);
            }
        }
    }
    for figlio in &nodo.children {
        let nome = figlio.name.as_str();
        if nome.ends_with("MatrixTransform") {
            if let Some(valore) = figlio.attr("Matrix") {
                if let Some(t) = Mat::from_xps(valore) {
                    matrice = matrice.mul(t);
                }
            }
        } else if nome.ends_with(".RenderTransform") || nome == "RenderTransform" {
            if let Some(valore) = figlio.attr("RenderTransform") {
                if let Some(t) = Mat::from_xps(valore) {
                    matrice = matrice.mul(t);
                }
            }
            if let Some(inner) = figlio.child("MatrixTransform") {
                if let Some(valore) = inner.attr("Matrix") {
                    if let Some(t) = Mat::from_xps(valore) {
                        matrice = matrice.mul(t);
                    }
                }
            }
        }
    }
    matrice
}

/// Matrice che porta il contenuto di un `VisualBrush` dal Viewbox al Viewport.
fn mappatura_visualbrush(nodo: &Node) -> Mat {
    let unita_assolute = nodo
        .attr("ViewportUnits")
        .map(|unita| unita.eq_ignore_ascii_case("Absolute"))
        .unwrap_or(false);
    if !unita_assolute {
        return Mat::IDENTITY;
    }
    let Some(viewbox) = nodo.attr("Viewbox").and_then(rettangolo) else {
        return Mat::IDENTITY;
    };
    let Some(viewport) = nodo.attr("Viewport").and_then(rettangolo) else {
        return Mat::IDENTITY;
    };
    let (vx0, vy0, vx1, vy1) = viewbox;
    let (px0, py0, px1, py1) = viewport;
    let larghezza = vx1 - vx0;
    let altezza = vy1 - vy0;
    if larghezza <= 0.0 || altezza <= 0.0 {
        return Mat::IDENTITY;
    }
    let scala_x = (px1 - px0) / larghezza;
    let scala_y = (py1 - py0) / altezza;
    Mat {
        a: scala_x,
        b: 0.0,
        c: 0.0,
        d: scala_y,
        e: px0 - vx0 * scala_x,
        f: py0 - vy0 * scala_y,
    }
}

fn chiave_risorsa(valore: &str) -> Option<String> {
    let valore = valore.trim();
    let interno = valore.strip_prefix('{')?.strip_suffix('}')?;
    let (tipo, chiave) = interno.split_once(' ')?;
    if tipo.eq_ignore_ascii_case("StaticResource") {
        Some(chiave.trim().to_string())
    } else {
        None
    }
}

fn rettangolo(testo: &str) -> Option<(f64, f64, f64, f64)> {
    let n = numeri(testo);
    if n.len() < 4 {
        return None;
    }
    Some((n[0], n[1], n[0] + n[2], n[1] + n[3]))
}

fn colore(testo: &str) -> Option<[f64; 3]> {
    let testo = testo.trim();
    if let Some(esadecimale) = testo.strip_prefix('#') {
        return colore_esadecimale(esadecimale);
    }
    let tinta = match testo.to_ascii_lowercase().as_str() {
        "black" => [0.0, 0.0, 0.0],
        "white" => [1.0, 1.0, 1.0],
        "red" => [1.0, 0.0, 0.0],
        "green" => [0.0, 0.5, 0.0],
        "blue" => [0.0, 0.0, 1.0],
        "yellow" => [1.0, 1.0, 0.0],
        "orange" => [1.0, 0.65, 0.0],
        "purple" => [0.5, 0.0, 0.5],
        "gray" | "grey" => [0.5, 0.5, 0.5],
        _ => return None,
    };
    Some(tinta)
}

fn colore_esadecimale(testo: &str) -> Option<[f64; 3]> {
    let cifre: String = testo
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase();
    let (alfa, rosso, verde, blu) = match cifre.len() {
        8 => (&cifre[0..2], &cifre[2..4], &cifre[4..6], &cifre[6..8]),
        6 => ("ff", &cifre[0..2], &cifre[2..4], &cifre[4..6]),
        3 => ("ff", &cifre[0..1], &cifre[1..2], &cifre[2..3]),
        _ => return None,
    };
    let componente = |valore: &str| -> Option<f64> {
        let valore = if valore.len() == 1 {
            format!("{valore}{valore}")
        } else {
            valore.to_string()
        };
        u8::from_str_radix(&valore, 16)
            .ok()
            .map(|v| f64::from(v) / 255.0)
    };
    if alfa == "00" {
        return None;
    }
    Some([componente(rosso)?, componente(verde)?, componente(blu)?])
}

fn array_larghezze(larghezze: &BTreeMap<u16, u32>) -> String {
    if larghezze.is_empty() {
        return String::new();
    }
    let chiavi: Vec<u16> = larghezze.keys().copied().collect();
    let mut gruppi: Vec<String> = Vec::new();
    let mut indice = 0;
    while indice < chiavi.len() {
        let inizio = chiavi[indice];
        let mut fine = inizio;
        while indice + 1 < chiavi.len() && chiavi[indice + 1] == fine.saturating_add(1) {
            indice += 1;
            fine = chiavi[indice];
        }
        let valori: Vec<String> = (inizio..=fine)
            .map(|gid| larghezze.get(&gid).copied().unwrap_or(0).to_string())
            .collect();
        gruppi.push(format!("{inizio} [{}]", valori.join(" ")));
        indice += 1;
    }
    format!("/W [{}]", gruppi.join(" "))
}

fn cmap_unicode(usati: &BTreeMap<u16, Option<char>>) -> String {
    let coppie: Vec<(u16, char)> = usati
        .iter()
        .filter_map(|(gid, carattere)| carattere.map(|c| (*gid, c)))
        .collect();
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for blocco in coppie.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", blocco.len()));
        for (gid, carattere) in blocco {
            cmap.push_str(&format!(
                "<{gid:04X}> <{}>\n",
                utf16_esadecimale(*carattere)
            ));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap
}

fn utf16_esadecimale(carattere: char) -> String {
    let mut buffer = [0u16; 2];
    carattere
        .encode_utf16(&mut buffer)
        .iter()
        .map(|unit| format!("{unit:04X}"))
        .collect()
}
