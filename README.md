<div align="center">
  <a href="https://andreax79.github.io/tabboz/"><img alt="Tabboz Simulator" src="https://github.com/andreax79/tabboz/assets/1288154/64269165-8958-4f3a-945a-82c3ffffd4de"/></a>
</div>
<br/>

**Tabboz Simulator — port in puro Rust per macOS.**

Il port 100% Rust (nessun codice C, nessuna DLL, nessuna macchina virtuale)
del classico italiano del 1997 *Tabboz Simulator* (Win32 + BWCC, Obscured
Truckware). Grafica e gameplay identici all'originale: stesso motore di
finestre stile Win95/BWCC disegnato a mano, stessi dialoghi, stessa logica
di gioco, stessi asset originali. Obiettivi del port: leggerezza e velocità
(rendering software su surface, nessuna dipendenza grafica pesante).

## Build

```shell
cargo build --release        # compila il binario
./bundle.sh                 # genera dist/Tabboz.app (bundle macOS + icona)
open dist/Tabboz.app        # gioca
```

Requisiti: macOS, toolchain Rust stabile. `bundle.sh` usa `iconutil` (incluso
in macOS) per generare l'icona del bundle a partire dall'icona originale del
gioco. Il binario buildato da solo (`cargo run --release`) funziona comunque.

La partita viene salvata in automatico in
`~/Library/Application Support/tabboz/tabboz.json` (apertura e salvataggio dal
menu *File*, autosave alla chiusura).

## Architettura

| Crate | Ruolo |
| --- | --- |
| `tabboz-res` | Codegen: dialoghi `.rc`, menu e stringhe originali → moduli Rust |
| `tabboz-win` | Motore UI Win95/BWCC: frame, chrome, controlli, font, decodifiche BMP/ICO (anche RLE4/RLE8), rendering pixel-perfect |
| `tabboz-core` | Logica di gioco portata 1:1 dal C originale: stato, eventi, calendario, materie, scooter, tipa... |
| `tabboz-app` | winit + softbuffer, thread di gioco con semantica modale, audio (rodio, WAV originali), persistenza JSON, bundle |

Principi di fedeltà all'originale:

- I dialoghi sono **finestre reali del sistema** (come `DialogBox` in Win32):
  dimensioni esatte del template, coordinate logiche 1 DLU = 2 px.
- Il font è il "Pixelated MS Sans Serif" di 98.css, rasterizzato con AA
  grayscale a risoluzione fisica e curva di contrasto calibrata (niente
  threshold binari: la resa è quella dell'originale hintato).
- I bitmap-button BWCC seguono le stesse regole del port web di riferimento
  (Novantotto): BorBtn OK/Cancel → face 1001/1002, BorBtn BS_OWNERDRAW →
  bitmap id+1000 stirata al rect del controllo.
- `resources/dialogs/html/` (il port web Novantotto) resta nel repo come
  oracolo visivo per confronti 1:1.

## Differenze note rispetto all'originale Win32

- Il timer "iconizzato" (avanzamento del giorno a finestra ridotta a icona
  per ~13-21 minuti) non è implementato; i giorni avanzano con le azioni
  di gioco come nel resto dell'originale.
- Il dialogo *Network* (easter egg finger server) non è raggiungibile.
- L'opzione *Enable Timer* del pannello Configuration è visiva.
- "Spegni il computer ed esci di casa" esce dal gioco senza tentare lo
  shutdown del Mac (l'originale chiamava `ExitWindows`).

## Crediti e licenza

Gioco originale: **Tabboz Simulator** — Copyright (C) 1997-2001 by Obscured
Truckware (Andrea Bonomi, Emanuele Caccialanza, Daniele Gazzarri). Questo
port riproduce il gioco partendo dal suo codice sorgente (commit storico) e
dai suoi asset originali.

[Novantotto](https://github.com/andreax79/novantotto) e
[98.css](https://github.com/jdan/98.css) sono stati la referenza tecnica per
la resa visiva del chrome Win95.

[![GPL 3](https://www.gnu.org/graphics/gplv3-or-later.png)](https://www.gnu.org/licenses/gpl-3.0.en.html)
