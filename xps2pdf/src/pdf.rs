//! Scrittura di un documento PDF 1.7 minimale ma valido.

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

/// Accumulatore di oggetti PDF con tabelle `xref` classiche.
pub struct Pdf {
    oggetti: Vec<Vec<u8>>,
}

impl Default for Pdf {
    fn default() -> Self {
        Self::new()
    }
}

impl Pdf {
    pub fn new() -> Self {
        Pdf {
            oggetti: Vec::new(),
        }
    }

    /// Prenota un identificativo oggetto, da riempire in seguito con `put`.
    pub fn alloc(&mut self) -> u32 {
        self.oggetti.push(Vec::new());
        self.oggetti.len() as u32
    }

    pub fn put(&mut self, id: u32, corpo: Vec<u8>) {
        let indice = (id - 1) as usize;
        if indice >= self.oggetti.len() {
            self.oggetti.resize(indice + 1, Vec::new());
        }
        self.oggetti[indice] = corpo;
    }

    pub fn add(&mut self, corpo: Vec<u8>) -> u32 {
        let id = self.alloc();
        self.put(id, corpo);
        id
    }

    /// Corpo di un oggetto flusso compresso con Deflate.
    pub fn stream_body(dict_extra: &str, dati: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        let compresso = match encoder.write_all(dati) {
            Ok(()) => encoder.finish().unwrap_or_else(|_| dati.to_vec()),
            Err(_) => dati.to_vec(),
        };
        let mut corpo = format!(
            "<< /Length {} /Filter /FlateDecode{} >>\nstream\n",
            compresso.len(),
            dict_extra
        )
        .into_bytes();
        corpo.extend_from_slice(&compresso);
        corpo.extend_from_slice(b"\nendstream");
        corpo
    }

    /// Aggiunge un oggetto flusso compresso e ne restituisce l'identificativo.
    pub fn add_stream(&mut self, dict_extra: &str, dati: &[u8]) -> u32 {
        let corpo = Pdf::stream_body(dict_extra, dati);
        self.add(corpo)
    }

    pub fn finish(self, radice: u32, info: Option<u32>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n");
        let mut offset = Vec::with_capacity(self.oggetti.len());
        for (indice, corpo) in self.oggetti.iter().enumerate() {
            offset.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", indice + 1).as_bytes());
            out.extend_from_slice(corpo);
            out.extend_from_slice(b"\nendobj\n");
        }
        let posizione_xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", self.oggetti.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for posizione in &offset {
            out.extend_from_slice(format!("{posizione:010} 00000 n \n").as_bytes());
        }
        let mut trailer = format!(
            "trailer\n<< /Size {} /Root {} 0 R",
            self.oggetti.len() + 1,
            radice
        );
        if let Some(id) = info {
            trailer.push_str(&format!(" /Info {id} 0 R"));
        }
        trailer.push_str(" >>\nstartxref\n");
        out.extend_from_slice(trailer.as_bytes());
        out.extend_from_slice(format!("{posizione_xref}\n%%EOF\n").as_bytes());
        out
    }
}
