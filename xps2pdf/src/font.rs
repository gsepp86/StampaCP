//! Caricamento dei font incorporati nel pacchetto XPS.
//!
//! I font XPS possono essere memorizzati come `.odttf`: i primi 32 byte sono
//! offuscati con il GUID che forma il nome della parte. L'operazione è un
//! semplice XOR, quindi la stessa procedura ripristina il font originale.

use ttf_parser::{Face, GlyphId};

pub struct FontInfo {
    pub data: Vec<u8>,
    pub postscript_name: String,
    pub units_per_em: f64,
    pub ascent: f64,
    pub descent: f64,
    pub cap_height: f64,
    pub bbox: [f64; 4],
    pub italic_angle: f64,
    pub flags: u32,
}

impl FontInfo {
    /// Carica il font provando prima a de-offuscare la parte `.odttf`.
    pub fn load(grezzo: &[u8], nome_parte: &str) -> Option<FontInfo> {
        let deoffuscato = deoffusca_odttf(nome_parte, grezzo);
        let mut candidati = vec![deoffuscato];
        candidati.push(grezzo.to_vec());
        for data in candidati {
            if let Some(info) = FontInfo::parse(data) {
                return Some(info);
            }
        }
        None
    }

    fn parse(data: Vec<u8>) -> Option<FontInfo> {
        let face = Face::parse(&data, 0).ok()?;
        let units_per_em = face.units_per_em() as f64;
        if units_per_em <= 0.0 {
            return None;
        }
        let scala = 1000.0 / units_per_em;
        let bbox = face.global_bounding_box();
        let cap_height = face
            .capital_height()
            .map(f64::from)
            .unwrap_or_else(|| f64::from(face.ascender()) * 0.7);
        let nome = nome_postscript(&face)
            .map(|n| normalizza_nome_font(&n))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "StampaCPFont".to_string());
        let mut flags = 4; // Symbolic: il font è usato tramite CID, non per codifica testo.
        if face.is_italic() {
            flags |= 64;
        }
        let ascent = f64::from(face.ascender()) * scala;
        let descent = f64::from(face.descender()) * scala;
        let riquadro = [
            f64::from(bbox.x_min) * scala,
            f64::from(bbox.y_min) * scala,
            f64::from(bbox.x_max) * scala,
            f64::from(bbox.y_max) * scala,
        ];
        // Il prestito di `data` termina qui: i valori sono già stati calcolati.
        Some(FontInfo {
            data,
            postscript_name: nome,
            units_per_em,
            ascent,
            descent,
            cap_height: cap_height * scala,
            bbox: riquadro,
            italic_angle: 0.0,
            flags,
        })
    }

    pub fn glyph_index(&self, carattere: char) -> Option<u16> {
        let face = Face::parse(&self.data, 0).ok()?;
        face.glyph_index(carattere).map(|g| g.0)
    }

    /// Larghezza di avanzamento in unità del font.
    pub fn advance(&self, gid: u16) -> f64 {
        let Ok(face) = Face::parse(&self.data, 0) else {
            return 0.0;
        };
        face.glyph_hor_advance(GlyphId(gid))
            .map(f64::from)
            .unwrap_or(0.0)
    }

    /// Larghezza per la tabella `/W` del PDF, in millesimi di em.
    pub fn width_1000(&self, gid: u16) -> u32 {
        let larghezza = self.advance(gid) * 1000.0 / self.units_per_em;
        larghezza.round().clamp(0.0, 65535.0) as u32
    }
}

fn nome_postscript(face: &Face<'_>) -> Option<String> {
    for nome in face.names() {
        if nome.name_id == ttf_parser::name_id::POST_SCRIPT_NAME {
            if let Some(testo) = nome.to_string() {
                return Some(testo);
            }
        }
    }
    for nome in face.names() {
        if nome.name_id == ttf_parser::name_id::FAMILY {
            if let Some(testo) = nome.to_string() {
                return Some(testo.replace(' ', ""));
            }
        }
    }
    None
}

fn normalizza_nome_font(nome: &str) -> String {
    nome.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '.'))
        .take(60)
        .collect()
}

/// Ripristina i primi 32 byte di un font offuscato `.odttf`.
pub fn deoffusca_odttf(nome_parte: &str, dati: &[u8]) -> Vec<u8> {
    let stem = nome_parte
        .rsplit('/')
        .next()
        .unwrap_or(nome_parte)
        .split('.')
        .next()
        .unwrap_or("");
    let cifre: String = stem
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase();
    if cifre.len() != 32 || dati.len() < 32 {
        return dati.to_vec();
    }
    let mut chiave = [0u8; 16];
    for (indice, byte) in chiave.iter_mut().enumerate() {
        let coppia = &cifre[indice * 2..indice * 2 + 2];
        // Il GUID è memorizzato in ordine inverso rispetto al nome della parte.
        *byte = u8::from_str_radix(coppia, 16).unwrap_or(0);
    }
    chiave.reverse();
    let mut out = dati.to_vec();
    for (indice, byte) in out.iter_mut().enumerate().take(32) {
        *byte ^= chiave[indice % 16];
    }
    out
}
