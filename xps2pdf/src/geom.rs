//! Matrici affini e geometrie vettoriali usate dalle pagine XPS.
//!
//! Le coordinate XPS sono espresse in unità di 1/96 di pollice con l'asse Y
//! rivolto verso il basso; il PDF usa punti tipografici (1/72 di pollice) con
//! l'asse Y rivolto verso l'alto. La conversione avviene con una matrice
//! applicata all'inizio del flusso di contenuto di ogni pagina.

use std::f64::consts::PI;

pub type Point = (f64, f64);

/// Matrice affine: (x, y) -> (a·x + c·y + e, b·x + d·y + f).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Mat {
    pub const IDENTITY: Mat = Mat {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Compone due matrici: prima `altro`, poi `self`.
    pub fn mul(self, altro: Mat) -> Mat {
        Mat {
            a: self.a * altro.a + self.c * altro.b,
            b: self.b * altro.a + self.d * altro.b,
            c: self.a * altro.c + self.c * altro.d,
            d: self.b * altro.c + self.d * altro.d,
            e: self.a * altro.e + self.c * altro.f + self.e,
            f: self.b * altro.e + self.d * altro.f + self.f,
        }
    }

    /// Legge una matrice XPS (`m11 m12 m21 m22 offsetX offsetY`).
    pub fn from_xps(testo: &str) -> Option<Mat> {
        let n = numeri(testo);
        if n.len() < 6 {
            return None;
        }
        Some(Mat {
            a: n[0],
            b: n[1],
            c: n[2],
            d: n[3],
            e: n[4],
            f: n[5],
        })
    }

    /// Operandi per l'operatore PDF `cm`.
    pub fn operands(self) -> String {
        format!(
            "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
            self.a, self.b, self.c, self.d, self.e, self.f
        )
    }
}

/// Estrae tutti i numeri da una stringa (separatori: spazi e virgole).
pub fn numeri(testo: &str) -> Vec<f64> {
    testo
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f64>().ok())
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub enum Segment {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
}

/// Geometria di un elemento `Path`: segmenti già assoluti.
#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub segments: Vec<Segment>,
    pub even_odd: bool,
}

impl Geometry {
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Rettangolo di ingombro approssimato (include i punti di controllo).
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut visto = false;
        let mut aggiorna = |p: Point| {
            if p.0.is_finite() && p.1.is_finite() {
                min_x = min_x.min(p.0);
                min_y = min_y.min(p.1);
                max_x = max_x.max(p.0);
                max_y = max_y.max(p.1);
                visto = true;
            }
        };
        for segmento in &self.segments {
            match *segmento {
                Segment::Move(p) | Segment::Line(p) => aggiorna(p),
                Segment::Curve(c1, c2, p) => {
                    aggiorna(c1);
                    aggiorna(c2);
                    aggiorna(p);
                }
                Segment::Close => {}
            }
        }
        if visto {
            Some((min_x, min_y, max_x, max_y))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Comando {
    Move,
    MoveRel,
    Line,
    LineRel,
    HLine,
    HLineRel,
    VLine,
    VLineRel,
    Cubic,
    CubicRel,
    SCubic,
    SCubicRel,
    Quad,
    QuadRel,
    SQuad,
    SQuadRel,
    Arc,
    ArcRel,
    Close,
    EvenOdd,
    NonZero,
}

impl Comando {
    fn is_relativo(self) -> bool {
        matches!(
            self,
            Comando::MoveRel
                | Comando::LineRel
                | Comando::HLineRel
                | Comando::VLineRel
                | Comando::CubicRel
                | Comando::SCubicRel
                | Comando::QuadRel
                | Comando::SQuadRel
                | Comando::ArcRel
        )
    }

    fn arita(self) -> usize {
        match self {
            Comando::Move | Comando::MoveRel | Comando::Line | Comando::LineRel => 2,
            Comando::HLine | Comando::HLineRel | Comando::VLine | Comando::VLineRel => 1,
            Comando::Cubic | Comando::CubicRel => 6,
            Comando::SCubic | Comando::SCubicRel | Comando::Quad | Comando::QuadRel => 4,
            Comando::SQuad | Comando::SQuadRel => 2,
            Comando::Arc | Comando::ArcRel => 7,
            _ => 0,
        }
    }
}

fn comando_da_carattere(carattere: char) -> Option<Comando> {
    Some(match carattere {
        'M' => Comando::Move,
        'm' => Comando::MoveRel,
        'L' => Comando::Line,
        'l' => Comando::LineRel,
        'H' => Comando::HLine,
        'h' => Comando::HLineRel,
        'V' => Comando::VLine,
        'v' => Comando::VLineRel,
        'C' => Comando::Cubic,
        'c' => Comando::CubicRel,
        'S' => Comando::SCubic,
        's' => Comando::SCubicRel,
        'Q' => Comando::Quad,
        'q' => Comando::QuadRel,
        'T' => Comando::SQuad,
        't' => Comando::SQuadRel,
        'A' => Comando::Arc,
        'a' => Comando::ArcRel,
        'Z' | 'z' => Comando::Close,
        _ => return None,
    })
}

enum Token {
    Comando(Comando),
    Numero(f64),
}

/// Tokenizza la proprietà `Data`: i comandi possono essere attaccati ai numeri
/// (es. `M0,0L816.32,0`), come accade nei file generati da Windows.
fn tokenizza(data: &str) -> Vec<Token> {
    let bytes = data.as_bytes();
    let mut token = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte.is_ascii_whitespace() || byte == b',' {
            i += 1;
            continue;
        }
        let carattere = byte as char;
        if let Some(comando) = comando_da_carattere(carattere) {
            token.push(Token::Comando(comando));
            i += 1;
            continue;
        }
        if carattere == 'F' || carattere == 'f' {
            // Regola di riempimento: F0 (even-odd) oppure F1 (nonzero).
            let mut fine = i + 1;
            while fine < bytes.len() && bytes[fine].is_ascii_digit() {
                fine += 1;
            }
            match data[i..fine].to_ascii_uppercase().as_str() {
                "F0" => {
                    token.push(Token::Comando(Comando::EvenOdd));
                    i = fine;
                    continue;
                }
                "F1" => {
                    token.push(Token::Comando(Comando::NonZero));
                    i = fine;
                    continue;
                }
                _ => {}
            }
        }

        let inizio = i;
        if byte == b'+' || byte == b'-' {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            let mut esponente = i + 1;
            if esponente < bytes.len() && (bytes[esponente] == b'+' || bytes[esponente] == b'-') {
                esponente += 1;
            }
            let cifre = esponente;
            while esponente < bytes.len() && bytes[esponente].is_ascii_digit() {
                esponente += 1;
            }
            if esponente > cifre {
                i = esponente;
            }
        }
        if i == inizio {
            i += 1;
            continue;
        }
        if let Ok(valore) = data[inizio..i].parse::<f64>() {
            token.push(Token::Numero(valore));
        }
    }
    token
}

/// Interpreta la proprietà `Data` di un `Path` (stesso mini-linguaggio di SVG).
pub fn parse_geometry(data: &str) -> Geometry {
    let token = tokenizza(data);
    let mut geometria = Geometry::default();
    let mut i = 0usize;
    let mut corrente: Point = (0.0, 0.0);
    let mut inizio: Point = (0.0, 0.0);
    let mut controllo_cubico: Option<Point> = None;
    let mut controllo_quadratico: Option<Point> = None;
    let mut comando: Option<Comando> = None;

    while i < token.len() {
        if let Token::Comando(nuovo) = token[i] {
            i += 1;
            match nuovo {
                Comando::EvenOdd => {
                    geometria.even_odd = true;
                    comando = None;
                }
                Comando::NonZero => {
                    geometria.even_odd = false;
                    comando = None;
                }
                Comando::Close => {
                    geometria.segments.push(Segment::Close);
                    corrente = inizio;
                    controllo_cubico = None;
                    controllo_quadratico = None;
                    comando = None;
                }
                altro => comando = Some(altro),
            }
            continue;
        }

        let Some(c) = comando else {
            i += 1;
            continue;
        };
        let arita = c.arita();
        if arita == 0 || i + arita > token.len() {
            i += 1;
            continue;
        }
        let mut valori = [0.0_f64; 7];
        let mut validi = true;
        for (k, valore) in valori.iter_mut().enumerate().take(arita) {
            match token[i + k] {
                Token::Numero(n) => *valore = n,
                Token::Comando(_) => {
                    validi = false;
                    break;
                }
            }
        }
        if !validi {
            comando = None;
            continue;
        }
        i += arita;

        let relativo = c.is_relativo();
        let punto = |x: f64, y: f64| -> Point {
            if relativo {
                (corrente.0 + x, corrente.1 + y)
            } else {
                (x, y)
            }
        };

        match c {
            Comando::Move | Comando::MoveRel => {
                let p = punto(valori[0], valori[1]);
                geometria.segments.push(Segment::Move(p));
                corrente = p;
                inizio = p;
                controllo_cubico = None;
                controllo_quadratico = None;
                // I numeri successivi, senza nuovo comando, sono linee.
                comando = Some(if relativo {
                    Comando::LineRel
                } else {
                    Comando::Line
                });
            }
            Comando::Line | Comando::LineRel => {
                let p = punto(valori[0], valori[1]);
                geometria.segments.push(Segment::Line(p));
                corrente = p;
                controllo_cubico = None;
                controllo_quadratico = None;
            }
            Comando::HLine | Comando::HLineRel => {
                let x = if relativo {
                    corrente.0 + valori[0]
                } else {
                    valori[0]
                };
                let p = (x, corrente.1);
                geometria.segments.push(Segment::Line(p));
                corrente = p;
                controllo_cubico = None;
                controllo_quadratico = None;
            }
            Comando::VLine | Comando::VLineRel => {
                let y = if relativo {
                    corrente.1 + valori[0]
                } else {
                    valori[0]
                };
                let p = (corrente.0, y);
                geometria.segments.push(Segment::Line(p));
                corrente = p;
                controllo_cubico = None;
                controllo_quadratico = None;
            }
            Comando::Cubic | Comando::CubicRel => {
                let c1 = punto(valori[0], valori[1]);
                let c2 = punto(valori[2], valori[3]);
                let p = punto(valori[4], valori[5]);
                geometria.segments.push(Segment::Curve(c1, c2, p));
                corrente = p;
                controllo_cubico = Some(c2);
                controllo_quadratico = None;
            }
            Comando::SCubic | Comando::SCubicRel => {
                let c1 = controllo_cubico
                    .map(|c| (2.0 * corrente.0 - c.0, 2.0 * corrente.1 - c.1))
                    .unwrap_or(corrente);
                let c2 = punto(valori[0], valori[1]);
                let p = punto(valori[2], valori[3]);
                geometria.segments.push(Segment::Curve(c1, c2, p));
                corrente = p;
                controllo_cubico = Some(c2);
                controllo_quadratico = None;
            }
            Comando::Quad | Comando::QuadRel => {
                let q = punto(valori[0], valori[1]);
                let p = punto(valori[2], valori[3]);
                geometria.segments.push(quadratica_a_cubica(corrente, q, p));
                corrente = p;
                controllo_quadratico = Some(q);
                controllo_cubico = None;
            }
            Comando::SQuad | Comando::SQuadRel => {
                let q = controllo_quadratico
                    .map(|c| (2.0 * corrente.0 - c.0, 2.0 * corrente.1 - c.1))
                    .unwrap_or(corrente);
                let p = punto(valori[0], valori[1]);
                geometria.segments.push(quadratica_a_cubica(corrente, q, p));
                corrente = p;
                controllo_quadratico = Some(q);
                controllo_cubico = None;
            }
            Comando::Arc | Comando::ArcRel => {
                let p = punto(valori[5], valori[6]);
                let punti = arco(
                    corrente,
                    valori[0].abs(),
                    valori[1].abs(),
                    valori[2],
                    valori[3] != 0.0,
                    valori[4] != 0.0,
                    p,
                );
                for punto_arco in punti {
                    geometria.segments.push(Segment::Line(punto_arco));
                }
                corrente = p;
                controllo_cubico = None;
                controllo_quadratico = None;
            }
            Comando::Close | Comando::EvenOdd | Comando::NonZero => {}
        }
    }

    geometria
}

fn quadratica_a_cubica(da: Point, controllo: Point, a: Point) -> Segment {
    let c1 = (
        da.0 + 2.0 / 3.0 * (controllo.0 - da.0),
        da.1 + 2.0 / 3.0 * (controllo.1 - da.1),
    );
    let c2 = (
        a.0 + 2.0 / 3.0 * (controllo.0 - a.0),
        a.1 + 2.0 / 3.0 * (controllo.1 - a.1),
    );
    Segment::Curve(c1, c2, a)
}

/// Converte un arco ellittico (parametrizzazione SVG) in una polilinea.
fn arco(
    da: Point,
    rx: f64,
    ry: f64,
    rotazione_gradi: f64,
    grande_arco: bool,
    senso_orario: bool,
    a: Point,
) -> Vec<Point> {
    if rx == 0.0 || ry == 0.0 || da == a {
        return vec![a];
    }
    let phi = rotazione_gradi.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (da.0 - a.0) / 2.0;
    let dy = (da.1 - a.1) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    let mut rx = rx;
    let mut ry = ry;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let fattore = lambda.sqrt();
        rx *= fattore;
        ry *= fattore;
    }

    let segno = if grande_arco != senso_orario {
        1.0
    } else {
        -1.0
    };
    let numeratore = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let denominatore = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let coefficiente = if denominatore == 0.0 {
        0.0
    } else {
        segno * (numeratore.max(0.0) / denominatore).sqrt()
    };
    let cxp = coefficiente * rx * y1p / ry;
    let cyp = -coefficiente * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (da.0 + a.0) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (da.1 + a.1) / 2.0;

    let angolo = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let prodotto = ux * vx + uy * vy;
        let modulo = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        if modulo == 0.0 {
            0.0
        } else {
            (prodotto / modulo).clamp(-1.0, 1.0).acos() * (ux * vy - uy * vx).signum()
        }
    };

    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let inizio = angolo(1.0, 0.0, ux, uy);
    let mut delta = angolo(ux, uy, vx, vy);
    if !senso_orario && delta > 0.0 {
        delta -= 2.0 * PI;
    } else if senso_orario && delta < 0.0 {
        delta += 2.0 * PI;
    }

    let passi = 24;
    let mut punti = Vec::with_capacity(passi);
    for k in 1..=passi {
        let t = inizio + delta * (k as f64) / (passi as f64);
        let (sin_t, cos_t) = t.sin_cos();
        punti.push((
            cx + rx * cos_phi * cos_t - ry * sin_phi * sin_t,
            cy + rx * sin_phi * cos_t + ry * cos_phi * sin_t,
        ));
    }
    if let Some(ultimo) = punti.last_mut() {
        *ultimo = a;
    }
    punti
}
