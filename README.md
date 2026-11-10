# Stampante virtuale (driver v4 + Print Support App) in Rust per Windows

Progetto di stampante PDF virtuale che conserva il testo, con preferenze gestite da una finestra dedicata.

Stampante virtuale che migliora la qualità delle tue stampe, con funzioni avanzate e testo sempre ricercabile nei PDF. Creata da Peppe CP con l’aiuto della IA.

## Se vuoi contribuire al mio lavoro ti lascio in basso il pulsante per contribuire anche con una piccola donazione simbolica visto che una app così utile gratis per sempre non la trovi ovunque apprezzo il gesto ;-)

[![Donate via PayPal](https://img.shields.io/badge/Donate-PayPal-blue.svg?logo=paypal)](https://paypal.me/PeppeCP)

## Struttura

| Crate | Ruolo | Stato |
|---|---|---|
| `xps2pdf` | Converte XPS in PDF con testo vettoriale | scheletro (fase 1) |
| `driver` | Driver v4 UMDF che riceve i lavori di stampa | da fare (fase 3) |
| `psa` | Print Support App con la finestra delle preferenze | da fare (fase 4) |
| `printticket` | Schema e lettura del PrintTicket (XML) | da fare (fase 2) |

## Piano

1. Convertitore XPS -> PDF (portabile, si costruisce anche su Linux).
2. Schema PrintTicket minimo: copie, colore, fronte/retro, formato carta.
3. Driver v4 UMDF che richiama il convertitore (solo Windows, con WDK).
4. Print Support App con la finestra delle preferenze.

## Note

- Il driver e la PSA richiedono Windows 10/11, Windows SDK e WDK.
- Per l'installazione in locale serve la modalità di test per la firma; per la distribuzione serve un certificato di firma del codice.
