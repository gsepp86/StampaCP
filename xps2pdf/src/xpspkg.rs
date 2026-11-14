//! Lettura di un pacchetto XPS (archivio ZIP) e parsing XML in un albero leggero.

use crate::ConvertError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    /// Valore di un attributo con nome esatto.
    pub fn attr(&self, nome: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(chiave, _)| chiave == nome)
            .map(|(_, valore)| valore.as_str())
    }

    /// Valore di un attributo ignorando l'eventuale prefisso di namespace.
    pub fn attr_locale(&self, nome: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(chiave, _)| {
                chiave == nome
                    || chiave
                        .rsplit_once(':')
                        .map(|(_, locale)| locale == nome)
                        .unwrap_or(false)
            })
            .map(|(_, valore)| valore.as_str())
    }

    /// Primo figlio con il nome indicato.
    pub fn child(&self, nome: &str) -> Option<&Node> {
        self.children.iter().find(|figlio| figlio.name == nome)
    }

    /// Scorre ricorsivamente tutti i discendenti (escluso `self`).
    pub fn descendants(&self) -> Vec<&Node> {
        let mut risultato = Vec::new();
        let mut coda: Vec<&Node> = self.children.iter().collect();
        while let Some(nodo) = coda.pop() {
            risultato.push(nodo);
            coda.extend(nodo.children.iter());
        }
        risultato
    }
}

/// Normalizza il nome di una parte del pacchetto (senza `/` iniziale, minuscolo,
/// con i caratteri percent-encoded decodificati).
pub fn normalizza_nome(nome: &str) -> String {
    let decodificato = percent_decode(nome.trim_start_matches('/'));
    decodificato.replace('\\', "/").to_lowercase()
}

/// Risolve un URI relativo alla parte che lo contiene.
pub fn risolvi_uri(parte_base: &str, uri: &str) -> String {
    let uri = uri.trim();
    if let Some(assoluto) = uri.strip_prefix('/') {
        return normalizza_nome(assoluto);
    }
    let mut segmenti: Vec<&str> = parte_base.split('/').collect();
    segmenti.pop();
    for pezzo in uri.split('/') {
        match pezzo {
            "" | "." => {}
            ".." => {
                segmenti.pop();
            }
            altro => segmenti.push(altro),
        }
    }
    normalizza_nome(&segmenti.join("/"))
}

fn percent_decode(testo: &str) -> String {
    let bytes = testo.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let coppia = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(valore) = u8::from_str_radix(coppia, 16) {
                out.push(valore);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Contenuto di un pacchetto XPS già letto in memoria.
pub struct Package {
    parti: BTreeMap<String, Vec<u8>>,
    /// Parti "interleaved": una singola parte suddivisa in pezzi numerati
    /// (`1.fpage/[0].piece`, `[1].piece`, …).
    interleaved: BTreeMap<String, Vec<(u32, Vec<u8>)>>,
}

impl Package {
    pub fn open(bytes: &[u8]) -> Result<Package, ConvertError> {
        let mut archivio = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| ConvertError::Malformed(format!("archivio XPS non valido: {e}")))?;
        let mut parti = BTreeMap::new();
        let mut interleaved: BTreeMap<String, Vec<(u32, Vec<u8>)>> = BTreeMap::new();
        for indice in 0..archivio.len() {
            let mut voce = archivio
                .by_index(indice)
                .map_err(|e| ConvertError::Malformed(format!("parte XPS illeggibile: {e}")))?;
            if voce.is_dir() {
                continue;
            }
            let nome = normalizza_nome(voce.name());
            let mut dati = Vec::new();
            voce.read_to_end(&mut dati)
                .map_err(|e| ConvertError::Malformed(format!("parte XPS corrotta: {e}")))?;
            if let Some((base, numero)) = pezzo_interleaved(&nome) {
                interleaved.entry(base).or_default().push((numero, dati));
            } else {
                parti.insert(nome, dati);
            }
        }
        if parti.is_empty() {
            return Err(ConvertError::Malformed(
                "il file XPS non contiene parti".into(),
            ));
        }
        Ok(Package { parti, interleaved })
    }

    pub fn part(&self, nome: &str) -> Option<&[u8]> {
        self.parti.get(&normalizza_nome(nome)).map(|d| d.as_slice())
    }

    /// Contenuto di una parte, ricomponendo i pezzi delle parti "interleaved".
    pub fn part_bytes(&self, nome: &str) -> Option<Vec<u8>> {
        let chiave = normalizza_nome(nome);
        if let Some(dati) = self.parti.get(&chiave) {
            return Some(dati.clone());
        }
        let pezzi = self.interleaved.get(&chiave)?;
        let mut ordinati: Vec<&(u32, Vec<u8>)> = pezzi.iter().collect();
        ordinati.sort_by_key(|(numero, _)| *numero);
        let mut dati = Vec::new();
        for (_, pezzo) in ordinati {
            dati.extend_from_slice(pezzo);
        }
        Some(dati)
    }

    /// Nomi di tutte le parti, comprese le parti interleaved ricomponibili.
    pub fn part_names(&self) -> impl Iterator<Item = &String> {
        self.parti.keys().chain(self.interleaved.keys())
    }
}

/// Riconosce una parte interleaved: `cartella/1.fpage/[0].piece`.
fn pezzo_interleaved(nome: &str) -> Option<(String, u32)> {
    if !nome.ends_with(".piece") {
        return None;
    }
    let (base, pezzo) = nome.split_once("/[")?;
    let numero = pezzo.split(']').next()?;
    Some((base.to_string(), numero.parse().ok()?))
}

fn nodo_da_evento(evento: &BytesStart<'_>) -> Result<Node, ConvertError> {
    let nome = String::from_utf8_lossy(evento.name().as_ref()).to_string();
    let mut attrs = Vec::new();
    for attributo in evento.attributes() {
        let attributo = attributo
            .map_err(|e| ConvertError::Malformed(format!("attributo XML non valido: {e}")))?;
        let chiave = String::from_utf8_lossy(attributo.key.as_ref()).to_string();
        let valore = attributo
            .unescape_value()
            .map_err(|e| ConvertError::Malformed(format!("valore XML non valido: {e}")))?
            .to_string();
        attrs.push((chiave, valore));
    }
    Ok(Node {
        name: nome,
        attrs,
        children: Vec::new(),
    })
}

/// Le parti XML di un pacchetto XPS possono essere codificate in UTF-16;
/// quick-xml lavora in UTF-8, quindi la codifica viene normalizzata prima.
fn xml_a_utf8(xml: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    use std::borrow::Cow;
    let le = xml.starts_with(&[0xFF, 0xFE]) || (xml.len() > 1 && xml[0] == b'<' && xml[1] == 0);
    let be = xml.starts_with(&[0xFE, 0xFF]) || (xml.len() > 1 && xml[0] == 0 && xml[1] == b'<');
    if !le && !be {
        return Cow::Borrowed(xml);
    }
    let inizio = if xml.starts_with(&[0xFF, 0xFE]) || xml.starts_with(&[0xFE, 0xFF]) {
        2
    } else {
        0
    };
    let unita: Vec<u16> = xml[inizio..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|coppia| {
            if le {
                u16::from_le_bytes(*coppia)
            } else {
                u16::from_be_bytes(*coppia)
            }
        })
        .collect();
    Cow::Owned(String::from_utf16_lossy(&unita).into_bytes())
}

/// Costruisce un albero di nodi dal documento XML indicato.
pub fn parse_dom(xml: &[u8]) -> Result<Node, ConvertError> {
    let xml = xml_a_utf8(xml);
    let mut reader = Reader::from_reader(xml.as_ref());
    reader.config_mut().trim_text(true);
    let mut pila: Vec<Node> = Vec::new();
    let mut radice: Option<Node> = None;
    let mut buffer = Vec::new();

    loop {
        let evento = reader
            .read_event_into(&mut buffer)
            .map_err(|e| ConvertError::Malformed(format!("XML XPS non valido: {e}")))?;
        match evento {
            Event::Start(inizio) => pila.push(nodo_da_evento(&inizio)?),
            Event::Empty(inizio) => {
                let nodo = nodo_da_evento(&inizio)?;
                match pila.last_mut() {
                    Some(padre) => padre.children.push(nodo),
                    None => {
                        if radice.is_none() {
                            radice = Some(nodo);
                        }
                    }
                }
            }
            Event::End(_) => {
                if let Some(nodo) = pila.pop() {
                    match pila.last_mut() {
                        Some(padre) => padre.children.push(nodo),
                        None => {
                            if radice.is_none() {
                                radice = Some(nodo);
                            }
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    radice.ok_or_else(|| ConvertError::Malformed("documento XML senza radice".into()))
}
