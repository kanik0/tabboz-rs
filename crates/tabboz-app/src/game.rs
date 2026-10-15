// The game thread: owns the GameState and implements every dialog handler
// of the original game (main window, famiglia, scuola, disco, negozi,
// tabaccaio, palestra, compagnia, about, personal info, spegnimi).
//
// Every handler is a faithful port of the corresponding WM_COMMAND switch
// of the C sources, with evento() (the "game turn") called exactly where
// the original called it.

use std::sync::mpsc::Receiver;

use tabboz_core::calendar::{self, SETTIMANA};
use tabboz_core::events::evento;
use tabboz_core::money::mostra_soldi;
use tabboz_core::state::{GameState, MATERIE};
use tabboz_core::{Interact, Rng};

use crate::ui::{GameAction, MainSnapshot, TabbozSnapshot, UiBridge, UiRequest};

/// Shop item tables (vestiti.c STSCOOTER reuse: speed/mass/xxx/fama/...).
/// Only cc (fama min for disco), fama, prezzo are meaningful here:
/// (fama bonus, prezzo in migliaia di lire).
struct ShopItem {
    fama: i32,
    prezzo: i64,
}

/// VestitiMem of vestiti.c (index 0 unused): 1-6 giubbotti, 7-10 pantaloni,
/// 11-17 scarpe.
const VESTITI_MEM: [ShopItem; 18] = [
    ShopItem { fama: 0, prezzo: 0 },    // 0: unused
    ShopItem { fama: 8, prezzo: 348 },  // 1: Giubbotto "Fatiscenza"
    ShopItem { fama: 9, prezzo: 378 },
    ShopItem { fama: 7, prezzo: 298 },
    ShopItem { fama: 8, prezzo: 248 },  // 4: Giacca di pelle
    ShopItem { fama: 9, prezzo: 378 },  // 5: Fatiscenza verde
    ShopItem { fama: 10, prezzo: 418 }, // 6: Fatiscenza bianco
    ShopItem { fama: 3, prezzo: 90 },   // 7: Pantaloni gessati
    ShopItem { fama: 5, prezzo: 170 }, // 8: Pantaloni tuta
    ShopItem { fama: 6, prezzo: 248 }, // 9: Pantaloni in plastika
    ShopItem { fama: 5, prezzo: 190 }, // 10: Pantaloni scacchiera
    ShopItem { fama: 4, prezzo: 122 }, // 11: Scarpe da tabbozzi
    ShopItem { fama: 6, prezzo: 220 }, // 12: Buffalo
    ShopItem { fama: 2, prezzo: 58 },  // 13
    ShopItem { fama: 4, prezzo: 142 }, // 14
    ShopItem { fama: 4, prezzo: 142 }, // 15
    ShopItem { fama: 5, prezzo: 166 }, // 16
    ShopItem { fama: 6, prezzo: 230 }, // 17: Nuove Buffalo
];

/// DiscoMem of disco.c (index 0 unused): cc = figosita' minima, fama = incremento,
/// xxx = incremento reputazione, mass = giorno di chiusura, prezzo.
struct Disco {
    fuori_porta: bool,
    fama: i32,
    fama_min: i32,
    rep: i32,
    chiusura: i32,
    prezzo: i64,
}

const DISCO_MEM: [Disco; 9] = [
    Disco { fuori_porta: false, fama: 0, fama_min: 0, rep: 0, chiusura: 0, prezzo: 0 },
    Disco { fuori_porta: false, fama: 30, fama_min: 2, rep: 15, chiusura: 1, prezzo: 36 },
    Disco { fuori_porta: false, fama: 0, fama_min: 1, rep: 7, chiusura: 4, prezzo: 26 },
    Disco { fuori_porta: false, fama: 0, fama_min: 1, rep: 8, chiusura: 1, prezzo: 30 },
    Disco { fuori_porta: false, fama: 35, fama_min: 3, rep: 15, chiusura: 1, prezzo: 36 },
    Disco { fuori_porta: false, fama: 0, fama_min: 2, rep: 6, chiusura: 3, prezzo: 26 },
    Disco { fuori_porta: false, fama: 0, fama_min: 2, rep: 5, chiusura: 2, prezzo: 22 },
    Disco { fuori_porta: false, fama: 0, fama_min: 3, rep: 8, chiusura: 1, prezzo: 30 },
    Disco { fuori_porta: true, fama: 2, fama_min: 9, rep: 7, chiusura: 0, prezzo: 36 },
];

/// PalestraMem of vestiti.c: prezzi abbonamenti (mese, sei mesi, anno, lampada).
const PALESTRA_MEM: [i64; 4] = [50, 270, 500, 14];

/// ScooterMem of scooter.c (for compagnia "gareggia"): speed and name.
const SCOOTER_MEM: [(i32, &str); 8] = [
    (0, "Nessuno scooter"),
    (65, "Magutty Firecow"),
    (75, "Honda F98"),
    (105, "Mizzubisci R200 Millenium"),
    (75, "Magutty Firecow+"),
    (100, "Magutty Firecow II"),
    (100, "Honda F98s"),
    (250, "Lexux LS400"),
];

const N_ATTIVITA: [&str; 7] = [
    "mancante",
    "funzionante",
    "ingrippato",
    "invasato",
    "parcheggiato",
    "sequestrato",
    "a secco",
];

/// Which handler drives the frame with a given token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handler {
    Main,
    Famiglia,
    Scuola,
    Disco,
    Negozi,
    Shop,
    Tabaccaio,
    Palestra,
    Compagnia,
    About,
    PersonalInfo,
    Logo,
    Spegnimi,
}

pub struct Game {
    pub state: GameState,
    pub rng: Rng,
    pub bridge: UiBridge,
    pub rx: Receiver<GameAction>,
    next_token: u64,
    /// Handler per open frame token.
    frames: Vec<(u64, Handler)>,
    /// Scuola: materia selezionata (1..=9).
    scelta: usize,
    /// Disco selection (1..=8) / shop selection / tabaccaio selection.
    selezione: usize,
    /// Spegnimi choice.
    shutdown: i32,
}

impl Game {
    pub fn new(state: GameState, rng: Rng, bridge: UiBridge, rx: Receiver<GameAction>) -> Self {
        Game {
            state,
            rng,
            bridge,
            rx,
            next_token: 1,
            frames: vec![(0, Handler::Main)],
            scelta: 1,
            selezione: 0,
            shutdown: 1,
        }
    }

    /// Runs until the app quits.
    pub fn run(&mut self) {
        self.update_main();
        while let Ok(action) = self.rx.recv() {
            match action {
                GameAction::Button { token, id } => self.on_button(token, id),
                GameAction::Command { id } => self.on_menu(id),
                GameAction::FrameClosed { token } => self.on_closed(token),
            }
        }
    }

    // ---- helpers ----------------------------------------------------------

    fn token(&mut self, handler: Handler) -> u64 {
        let t = self.next_token;
        self.next_token += 1;
        self.frames.push((t, handler));
        t
    }

    fn open(&mut self, handler: Handler, dlg_id: u32, texts: Vec<(i32, String)>, radios: Vec<i32>) {
        let t = self.token(handler);
        let _ = self.bridge.tx.send_event(UiRequest::ShowFrame {
            token: t,
            dlg_id,
            texts,
            radios,
            checks: vec![],
        });
    }

    fn close(&mut self, token: u64) {
        self.frames.retain(|(t, _)| *t != token);
        let _ = self.bridge.tx.send_event(UiRequest::CloseFrame { token });
    }

    fn set_text(&mut self, token: u64, id: i32, text: String) {
        let _ = self
            .bridge
            .tx
            .send_event(UiRequest::SetText { token, id, text });
    }

    #[allow(dead_code)] // used by later milestones
    fn set_radio(&mut self, token: u64, id: i32) {
        let _ = self.bridge.tx.send_event(UiRequest::SetRadio { token, id });
    }

    fn message(&mut self, title: &str, text: &str) {
        self.bridge.message(title, text);
    }

    fn confirm(&mut self, title: &str, text: &str) -> bool {
        self.bridge.confirm(title, text)
    }

    fn evento(&mut self) {
        evento(&mut self.state, &mut self.rng, &mut self.bridge);
        self.update_main();
    }

    /// AggiornaPrincipale: pushes the snapshot of the main window fields.
    fn update_main(&mut self) {
        let s = &self.state;
        let (nometipa, rapporti) = if s.rapporti != 0 {
            (s.nometipa.clone(), format!("{}/100", s.rapporti))
        } else {
            (" ".to_string(), String::new())
        };
        let (scooter_nome, scooter_stato) = if s.scooter.stato != -1 {
            (s.scooter.nome.clone(), format!("{}/100", s.scooter.stato))
        } else {
            (" ".to_string(), String::new())
        };
        let (tipa_label, rapporto_label) = if s.sesso == 'M' {
            ("Tipa".to_string(), "Rapporto con la tipa".to_string())
        } else {
            ("Tipo".to_string(), "Rapporto con il tipo".to_string())
        };
        let snapshot = MainSnapshot {
            nome: format!("{} {}", s.nome, s.cognome),
            soldi: mostra_soldi(s.soldi, s.euro),
            fama: format!("{}/100", s.fama),
            reputazione: format!("{}/100", s.reputazione),
            studio: format!("{}/100", s.studio),
            nometipa,
            rapporti,
            scooter_nome,
            scooter_stato,
            calendario: format!(
                "{} {} {}",
                SETTIMANA[(s.giornoset - 1) as usize],
                s.giorno,
                calendar::MESI[(s.mese - 1) as usize].0
            ),
            tipa_label,
            rapporto_label,
            tabboz: TabbozSnapshot {
                sesso: s.sesso,
                testa: s.current_testa,
                giubbotto: s.current_gibbotto,
                pantaloni: s.current_pantaloni,
                scarpe: s.current_scarpe,
            },
        };
        let _ = self.bridge.tx.send_event(UiRequest::UpdateMain { snapshot });
    }

    fn nomoney(&mut self, tipo: usize) {
        // nomoney() of zarrosim.c: DISCO=4, VESTITI=8, TABACCAIO=88, PALESTRA=89.
        let ao = self.state.ao();
        match tipo {
            4 => {
                self.message(
                    "Bella figura",
                    &format!(
                        "Appena entrat{ao} ti accorgi di non avere abbastanza soldi per pagare il biglietto.\n Un energumeno buttafuori ti deposita gentilmente in un cassonetto della spazzatura poco distante dalla discoteca."
                    ),
                );
                if self.state.reputazione > 3 {
                    self.state.reputazione -= 1;
                }
            }
            8 => {
                self.message(
                    "Bella figura",
                    &format!("Con cosa avresti intenzione di pagare, stronzett{ao} ??? Caramelle ???"),
                );
                if self.state.fama > 12 {
                    self.state.fama -= 3;
                }
                if self.state.reputazione > 4 {
                    self.state.reputazione -= 2;
                }
            }
            89 => {
                if self.state.sesso == 'M' {
                    self.message(
                        "Non hai abbastanza soldi...",
                        "L' enorme istruttore di bodybulding ultra-palestrato ti suona come una zampogna e ti scaraventa fuori dalla palestra.",
                    );
                } else {
                    self.message(
                        "Non hai abbastanza soldi...",
                        "L' enorme istruttore di bodybulding ultra-palestrato ti scaraventa fuori dalla palestra.",
                    );
                }
                if self.state.reputazione > 6 {
                    self.state.reputazione -= 5;
                }
            }
            _ => {
                self.message("Non hai abbastanza soldi...", "Torna quando avrai piu' soldi...");
            }
        }
    }

    fn not_ported(&mut self, what: &str) {
        self.message(
            "Tabboz Simulator",
            &format!("\"{what}\" non e' ancora stato portato: arrivera' nella prossima milestone."),
        );
    }

    // ---- event routing ----------------------------------------------------

    fn handler_of(&self, token: u64) -> Option<Handler> {
        self.frames.iter().find(|(t, _)| *t == token).map(|(_, h)| *h)
    }

    fn on_button(&mut self, token: u64, id: i32) {
        // OK/Cancel are handled per-dialog (the original switches on wParam
        // inside each dialog procedure; e.g. Disco's OK pays, About's OK
        // closes), so there is no global close here.
        match self.handler_of(token) {
            Some(Handler::Main) => self.on_main_button(id),
            Some(Handler::Famiglia) => self.on_famiglia(token, id),
            Some(Handler::Scuola) => self.on_scuola(token, id),
            Some(Handler::Disco) => self.on_disco(token, id),
            Some(Handler::Negozi) => self.on_negozi(token, id),
            Some(Handler::Shop) => self.on_shop(token, id),
            Some(Handler::Tabaccaio) => self.on_tabaccaio(token, id),
            Some(Handler::Palestra) => self.on_palestra(token, id),
            Some(Handler::Compagnia) => self.on_compagnia(token, id),
            Some(Handler::About) => self.on_about(token, id),
            Some(Handler::PersonalInfo) => self.close(token),
            Some(Handler::Logo) => self.close(token),
            Some(Handler::Spegnimi) => self.on_spegnimi(token, id),
            _ => {}
        }
    }

    fn on_closed(&mut self, token: u64) {
        if self.handler_of(token) == Some(Handler::Spegnimi) {
            // OK on Spegnimi: quit (radio 101) — radio 102 would "shutdown".
            if self.shutdown == 1 {
                let _ = self.bridge.tx.send_event(UiRequest::Quit);
            }
        }
        self.close(token);
    }

    // ---- main window ------------------------------------------------------

    fn on_menu(&mut self, id: u16) {
        match id {
            106 => {
                // Apri: loads the saved game (single autosave slot).
                match crate::save::load() {
                    Some(state) => {
                        self.state = state;
                        self.update_main();
                        self.message("Apri", "Partita caricata.");
                    }
                    None => self.message("Errore", "Nessuna partita salvata..."),
                }
            }
            107 => {
                crate::save::save(&self.state);
                self.message("Salva", "Partita salvata.");
            }
            108 => self.open_spegnimi(),
            120 => self.open_about(),
            121 => self.open(Handler::Logo, 12, vec![], vec![]),
            130 => self.not_ported("Scooter"),
            132 => self.open_disco(),
            133 => self.not_ported("Tipa"),
            134 => self.open_compagnia(),
            135 => self.open_famiglia(),
            136 => self.on_main_button(136),
            137 => self.not_ported("Lavoro"),
            139 => self.open_personalinfo(),
            140 => self.not_ported("Configuration"),
            141 => self.run_tabacchi(),
            142 => self.run_palestra(),
            143..=147 => self.run_vestiti(id),
            155 => self.not_ported("Telefonino"),
            _ => {}
        }
    }

    fn on_main_button(&mut self, id: i32) {
        match id {
            103 => self.open_personalinfo(), // BMPView click
            120 => self.open_about(),
            130 => self.not_ported("Scooter"),
            131 => self.open_negozi(),
            132 => self.open_disco(),
            133 => self.not_ported("Tipa"),
            134 => self.open_compagnia(),
            135 => self.open_famiglia(),
            136 => {
                if self.state.vacanza == 0 {
                    self.open_scuola();
                } else {
                    self.message(
                        "Scuola",
                        "Non puoi andare a scuola in un giorno di vacanza !",
                    );
                }
            }
            137 => self.not_ported("Lavoro"),
            _ => {}
        }
    }

    // ---- famiglia ---------------------------------------------------------

    fn open_famiglia(&mut self) {
        let soldi = mostra_soldi(self.state.soldi, self.state.euro);
        let paghetta = mostra_soldi(self.state.paghetta, self.state.euro);
        self.open(
            Handler::Famiglia,
            5,
            vec![(104, soldi), (105, paghetta)],
            vec![],
        );
    }

    fn on_famiglia(&mut self, token: u64, id: i32) {
        match id {
            101 => {
                // Chiedi aumento paghetta
                if self.state.studio > 40 {
                    let check = self.state.studio - self.state.paghetta as i32 + self.state.fortuna;
                    if check > (75 + self.rng.random(50)) && self.state.paghetta < 96 {
                        self.message(
                            "Aumento paghetta !",
                            &format!(
                                "Va bene... ti daremo {} di paghetta in piu'...",
                                mostra_soldi(5, self.state.euro)
                            ),
                        );
                        self.state.paghetta += 5;
                        self.evento();
                    } else {
                        self.message(
                            "Errore irrecuperabile",
                            "Vedi di scordartelo... Dovra' passare molto tempo prima che ti venga aumentata la paghetta...",
                        );
                        self.evento();
                    }
                } else {
                    self.message(
                        "Errore irrecuperabile",
                        "Quando andrai meglio a scuola, forse...",
                    );
                }
                let paghetta = mostra_soldi(self.state.paghetta, self.state.euro);
                self.set_text(token, 105, paghetta);
            }
            102 => {
                // Chiedi paghetta extra
                if self.state.studio >= 40 {
                    if self.state.attesa == 0 {
                        self.state.attesa = tabboz_core::ATTESA_MAX;
                        self.state.soldi += 10;
                        self.evento();
                    } else {
                        self.message(
                            "Non te li diamo",
                            "Ma insomma ! Non puoi continuamente chiedere soldi ! Aspetta ancora qualche giorno. Fai qualche cosa di economico nel frattempo...",
                        );
                        self.evento();
                    }
                } else {
                    let ao = self.state.ao();
                    self.message(
                        "Errore irrecuperabile",
                        &format!(
                            "Quando andrai meglio a scuola potrai tornare a chiederci dei soldi, non ora. Ma non lo sai che per la tua vita e' importante studiare, e dovresti impegnarti di piu', perche' quando ti impegni i risultati si vedono, solo che sei svogliat{ao} e non fai mai nulla, mi ricordo che quando ero giovane io era tutta un altra cosa... allora si' che i giovani studiavano..."
                        ),
                    );
                }
                let soldi = mostra_soldi(self.state.soldi, self.state.euro);
                self.set_text(token, 104, soldi);
            }
            103 => {
                // "Papa', mi dai 100000 lire ?"
                if self.state.sound_active {
                    self.bridge.play_sound(801);
                }
                self.message(
                    "Errore irrecuperabile",
                    "Non pensarci neanche lontanamente...",
                );
                self.evento();
            }
            _ => self.close(token),
        }
    }

    // ---- scuola -----------------------------------------------------------

    fn open_scuola(&mut self) {
        self.scelta = 1;
        let mut texts = self.scrivi_voti();
        texts.extend(self.materia_labels());
        self.open(Handler::Scuola, 10, texts, vec![110]);
    }

    fn materia_labels(&self) -> Vec<(i32, String)> {
        let nome = MATERIE[self.scelta];
        vec![
            (101, format!("Corrompi il prof di {nome}")),
            (
                102,
                if self.state.sesso == 'M' {
                    format!("Minaccia il prof di {nome}")
                } else {
                    format!("Seduci il prof di {nome}")
                },
            ),
            (103, format!("Studia {nome}")),
        ]
    }

    /// ScriviVoti + Aggiorna of scuola.c.
    fn scrivi_voti(&mut self) -> Vec<(i32, String)> {
        let mut texts = vec![
            (104, mostra_soldi(self.state.soldi, self.state.euro)),
            (105, format!("{}/100", self.state.reputazione)),
            (106, format!("{}/100", self.state.studio)),
        ];
        for i in 1..10 {
            texts.push((119 + i as i32, self.state.materie[i].to_string()));
        }
        texts
    }

    fn check_vacanza(&mut self) -> bool {
        if self.state.vacanza != 0 {
            self.message(
                "Scuola",
                "Oggi e' festa, la scuola e' chiusa, non si entra neanche per corrompere nessuno...",
            );
            false
        } else {
            true
        }
    }

    fn on_scuola(&mut self, token: u64, id: i32) {
        match id {
            110..=118 => {
                self.scelta = (id - 109) as usize;
                for (cid, text) in self.materia_labels() {
                    self.set_text(token, cid, text);
                }
            }
            101 => {
                // Corrompi i professori
                if self.check_vacanza() {
                    let i = 30 + self.rng.random(30) * 2;
                    let price = mostra_soldi(i as u64, self.state.euro);
                    if self.confirm(
                        "Corrompi i professori",
                        &format!("Ma... forse per {price} potrei dimenticare i tuoi ultimi compiti in classe..."),
                    ) {
                        if self.state.soldi >= i as u64 {
                            self.state.soldi -= i as u64;
                            let s = &mut self.state.materie[self.scelta];
                            *s += 3;
                            if *s > 10 {
                                *s = 10;
                            }
                        } else {
                            let s = &mut self.state.materie[self.scelta];
                            if *s < 2 {
                                *s -= 2;
                            }
                            self.message(
                                "Errore critico",
                                "Cosa ??? Prima cerchi di corrompermi, poi si scopre che non hai abbastanza soldi !!!",
                            );
                        }
                    }
                    self.evento();
                    for (cid, text) in self.scrivi_voti() {
                        self.set_text(token, cid, text);
                    }
                }
            }
            102 => {
                // Minaccia/Seduci i professori
                if self.check_vacanza() {
                    let success = if self.state.sesso == 'M' {
                        self.state.reputazione >= 30 || self.rng.random(10) < 1
                    } else {
                        self.state.fama >= 50 || self.rng.random(10) < 2
                    };
                    if success {
                        let s = &mut self.state.materie[self.scelta];
                        *s += 2;
                        if *s > 10 {
                            *s = 10;
                        }
                    } else {
                        self.bridge.play_sound(402);
                        if self.state.sesso == 'M' {
                            self.message(
                                "Bella figura",
                                "Cosa ??? Credi di farmi paura piccolo pezzettino di letame vestito da zarro... Deve ancora nasce chi puo' minacciarmi...",
                            );
                        } else {
                            self.message(
                                "Bella figura",
                                "Infastidito dalla tua presenza, il prof ti manda via a calci.",
                            );
                        }
                        if self.state.reputazione > 3 {
                            self.state.reputazione -= 2;
                        }
                        let s = &mut self.state.materie[self.scelta];
                        if *s > 2 {
                            *s -= 1;
                        }
                    }
                    self.evento();
                    for (cid, text) in self.scrivi_voti() {
                        self.set_text(token, cid, text);
                    }
                }
            }
            103 => {
                // Studia
                if self.check_vacanza() {
                    if self.state.reputazione > 10 {
                        self.state.reputazione -= 5;
                    }
                    if self.state.fama > 5 {
                        self.state.fama -= 1;
                    }
                    let s = &mut self.state.materie[self.scelta];
                    *s += 1;
                    if *s > 10 {
                        *s = 10;
                    }
                    self.state.calcola_studio();
                    self.evento();
                    for (cid, text) in self.scrivi_voti() {
                        self.set_text(token, cid, text);
                    }
                }
            }
            _ => self.close(token),
        }
    }

    // ---- disco ------------------------------------------------------------

    fn open_disco(&mut self) {
        let ao = self.state.ao();
        let soldi = mostra_soldi(self.state.soldi, self.state.euro);
        self.open(
            Handler::Disco,
            4,
            vec![(120, format!("O tip{ao}, in che disco andiamo ?")), (110, soldi)],
            vec![],
        );
    }

    fn on_disco(&mut self, token: u64, id: i32) {
        match id {
            101..=108 => {
                self.selezione = (id - 100) as usize;
                let numdisco = self.selezione;
                // LoadString(id) + price, like the original.
                let template = tabboz_win::strings::find(id as u16).unwrap_or("");
                let price = if self.state.sesso == 'M' {
                    DISCO_MEM[numdisco].prezzo
                } else {
                    DISCO_MEM[numdisco].prezzo - 10
                };
                let text = template.replace("%s", &mostra_soldi(price as u64, self.state.euro));
                self.set_text(token, 120, text);
            }
            1 => {
                self.paga_disco();
                self.close(token);
            }
            _ => self.close(token),
        }
    }

    /// PagaDisco of disco.c.
    fn paga_disco(&mut self) {
        let numdisco = self.selezione;
        if numdisco == 0 {
            return;
        }
        if DISCO_MEM[numdisco].fuori_porta && self.state.scooter.stato == -1 {
            self.message(
                "Discoteca fuori porta",
                "Senza lo scooter non puoi andare nelle discoteche fuori porta...",
            );
            self.evento();
            return;
        }
        if DISCO_MEM[numdisco].chiusura == self.state.giornoset {
            self.message(
                "Giorno di chiusura",
                "Un cartello recita che oggi e' il giorno di chiusura settimanale...",
            );
            return;
        }
        let prezzo = if self.state.sesso == 'M' {
            DISCO_MEM[numdisco].prezzo
        } else {
            DISCO_MEM[numdisco].prezzo - 10
        };
        if prezzo > self.state.soldi as i64 {
            self.nomoney(4); // DISCO
        } else if DISCO_MEM[numdisco].fama_min > self.state.fama && self.state.sesso == 'M' {
            self.bridge.play_sound(302);
            self.message(
                "Selezione all' ingresso",
                "Mi dispiace signore, conciato cosi', qui non puo' entrare...\nVenga vestito meglio la prossima volta, signore.",
            );
            if self.state.reputazione > 2 {
                self.state.reputazione -= 1;
            }
            if self.state.fama > 2 {
                self.state.fama -= 1;
            }
        } else {
            self.bridge.play_sound((303 + self.rng.random(3)) as u16);
            self.state.soldi -= prezzo as u64;
            self.state.fama += DISCO_MEM[numdisco].fama;
            self.state.reputazione += DISCO_MEM[numdisco].rep;
            if self.state.fama > 100 {
                self.state.fama = 100;
            }
            if self.state.reputazione > 100 {
                self.state.reputazione = 100;
            }
        }
        self.evento();
    }

    // ---- negozi + shops ---------------------------------------------------

    fn open_negozi(&mut self) {
        let soldi = mostra_soldi(self.state.soldi, self.state.euro);
        let mut texts = vec![(120, soldi)];
        // Offerta natalizia (vestiti.c WM_INITDIALOG).
        const COSTO_VESTITO_NATALIZIO: u64 = 58;
        if self.state.mese == 12
            && self.state.soldi >= COSTO_VESTITO_NATALIZIO
            && (14..25).contains(&self.state.giorno)
            && self.state.current_gibbotto != 19
            && self.state.current_pantaloni != 19
        {
            let price = mostra_soldi(COSTO_VESTITO_NATALIZIO, self.state.euro);
            if self.confirm(
                "Offerte Natalizie...",
                &format!("Vuoi comperare, per {price}, un meraviglioso vestito da Babbo Natale ?"),
            ) {
                self.state.current_gibbotto = 19;
                self.state.current_pantaloni = 19;
                self.state.soldi -= COSTO_VESTITO_NATALIZIO;
                let soldi = mostra_soldi(self.state.soldi, self.state.euro);
                texts[0] = (120, soldi);
                self.update_main();
            }
        }
        self.open(Handler::Negozi, 8, texts, vec![]);
    }

    fn on_negozi(&mut self, token: u64, id: i32) {
        match id {
            101..=105 => {
                self.run_vestiti((id + 59) as u16); // RunVestiti(wParam - 21) -> 80..84
                let soldi = mostra_soldi(self.state.soldi, self.state.euro);
                self.set_text(token, 120, soldi);
            }
            110 => {
                self.run_tabacchi();
                let soldi = mostra_soldi(self.state.soldi, self.state.euro);
                self.set_text(token, 120, soldi);
            }
            111 => {
                self.run_palestra();
                let soldi = mostra_soldi(self.state.soldi, self.state.euro);
                self.set_text(token, 120, soldi);
            }
            112 => self.not_ported("Telefonino"),
            _ => self.close(token),
        }
    }

    /// Menu Negozi commands (143-147) map to RunVestiti(BAUHOUSE..ALTRI).
    fn run_vestiti(&mut self, menu_id: u16) {
        let mut numero = match menu_id {
            143 => 80, // Bau House
            144 => 81, // Zoccolaro
            145 => 82, // Footsmocker
            146 => 83, // Footsmocker II
            147 => 84, // Blue Rider
            _ => 80,
        };
        // Versione femminile di "Bau House" e "Blue Rider".
        if self.state.sesso == 'F' {
            if numero == 80 {
                numero = 85;
            } else if numero == 84 {
                numero = 86;
            }
        }
        if self.state.vacanza == 2 {
            let ao = self.state.ao();
            self.message("Vestiti", &format!("Oh, tip{ao}... i negozi sono chiusi di festa..."));
            return;
        }
        if self.state.sound_active {
            if numero < 82 {
                self.bridge.play_sound(204);
            } else {
                self.bridge.play_sound(205);
            }
        }
        self.selezione = 0;
        // CompraQualcosa init: prices, soldi, fama.
        let mut texts = vec![
            (120, mostra_soldi(self.state.soldi, self.state.euro)),
            (121, format!("{}/100", self.state.fama)),
        ];
        for (i, item) in VESTITI_MEM.iter().enumerate().skip(1) {
            texts.push((100 + i as i32, mostra_soldi(item.prezzo as u64, self.state.euro)));
        }
        self.open(Handler::Shop, numero, texts, vec![]);
    }

    /// PagaQualcosa of vestiti.c.
    fn on_shop(&mut self, token: u64, id: i32) {
        match id {
            101..=119 => {
                self.selezione = (id - 100) as usize;
            }
            1 => {
                self.paga_qualcosa();
                self.close(token);
            }
            _ => {
                self.selezione = 0;
                self.close(token);
            }
        }
    }

    fn paga_qualcosa(&mut self) {
        let scelta = self.selezione;
        if scelta == 0 {
            return;
        }
        if VESTITI_MEM[scelta].prezzo > self.state.soldi as i64 {
            self.nomoney(8); // VESTITI
        } else {
            self.state.soldi -= VESTITI_MEM[scelta].prezzo as u64;
            match scelta {
                1..=6 => self.state.current_gibbotto = scelta as i32,
                7..=10 => self.state.current_pantaloni = (scelta - 6) as i32,
                11..=17 => self.state.current_scarpe = (scelta - 10) as i32,
                _ => {}
            }
            self.state.fama += VESTITI_MEM[scelta].fama;
            if self.state.fama > 100 {
                self.state.fama = 100;
            }
            self.update_main();
        }
        self.evento();
    }

    // ---- tabaccaio --------------------------------------------------------

    fn run_tabacchi(&mut self) {
        if self.state.vacanza == 2 {
            self.message(
                "Bar Tabacchi",
                "Rimani fisso a guardare la saracinesca del tabaccaio inrimediabilmente chiusa...",
            );
            return;
        }
        if self.state.sound_active {
            self.bridge.play_sound(203);
        }
        self.selezione = 0;
        let ao = self.state.ao();
        let texts = vec![
            (104, mostra_soldi(self.state.soldi, self.state.euro)),
            (105, self.state.sizze.to_string()),
            (106, format!("Che sigarette vuoi, ragazz{ao} ?")),
        ];
        self.open(Handler::Tabaccaio, 88, texts, vec![]);
    }

    /// SizeMem of vestiti.c: (condensato, nicotina*10, fama, prezzo, nome).
    const SIZE_MEM: [(i32, i32, i32, i64, &str); 24] = [
        (5, 5, 2, 6, "Barclay"),
        (8, 7, 1, 6, "Camel"),
        (7, 6, 2, 6, "Davidoff Superior Lights"),
        (7, 6, 2, 6, "Davidoff Mildnes"),
        (13, 9, 2, 6, "Davidoff Classic"),
        (9, 7, 1, 5, "Diana Blu"),
        (12, 9, 1, 5, "Diana Rosse"),
        (8, 7, 0, 6, "Dunhill Lights"),
        (7, 5, 0, 6, "Merit"),
        (14, 10, 0, 6, "Gauloises Blu"),
        (7, 6, 0, 6, "Gauloises Rosse"),
        (13, 10, 1, 5, "Unlucky Strike"),
        (9, 7, 1, 5, "Unlucky Strike Lights"),
        (8, 6, 2, 6, "Malborro Medium"),
        (12, 9, 2, 6, "Malborro Rosse"),
        (8, 6, 2, 6, "Malborro Lights"),
        (11, 10, 0, 5, "NS Rosse"),
        (9, 8, 0, 5, "NS Mild"),
        (9, 7, 1, 5, "Poll Mon Blu"),
        (12, 9, 1, 5, "Poll Mon Rosse"),
        (12, 10, 2, 6, "Philip Morris"),
        (4, 4, 2, 6, "Philip Morris Super Light"),
        (10, 9, 1, 5, "Armadis"),
        (11, 9, 0, 5, "Winston"),
    ];

    fn on_tabaccaio(&mut self, token: u64, id: i32) {
        match id {
            400..=423 => {
                let sel = (id - 400) as usize;
                self.selezione = sel + 1;
                let (cond, nico, _, _, nome) = Self::SIZE_MEM[sel];
                let desc = tabboz_win::strings::find((id + 1000) as u16).unwrap_or("");
                let text = if nico == 0 {
                    format!("{nome}\n{desc}")
                } else {
                    format!(
                        "{nome}\n{desc}Condensato: {cond} Nicotina: {}.{}",
                        nico / 10,
                        nico % 10
                    )
                };
                self.set_text(token, 106, text);
            }
            1 => {
                if self.selezione != 0 {
                    let sel = self.selezione - 1;
                    let (_, _, fama, prezzo, _) = Self::SIZE_MEM[sel];
                    if prezzo > self.state.soldi as i64 {
                        self.nomoney(88);
                    } else {
                        self.state.soldi -= prezzo as u64;
                        self.state.fama += fama;
                        if self.state.fama > 100 {
                            self.state.fama = 100;
                        }
                        self.state.sizze += 20;
                    }
                    // ART. 46 message (strings 600-607).
                    let i = 600 + self.rng.random(8);
                    let art = tabboz_win::strings::find(i as u16).unwrap_or("");
                    self.message("ART. 46 L. 29/12/1990 n. 428", art);
                    self.evento();
                }
                self.close(token);
            }
            _ => {
                self.selezione = 0;
                self.close(token);
            }
        }
    }

    // ---- palestra ---------------------------------------------------------

    fn run_palestra(&mut self) {
        if self.state.vacanza == 2 {
            self.message(
                "Palestra",
                "Il tuo fisico da atleta dovra' aspettare... visto che oggi la palestra e' chiusa...",
            );
            return;
        }
        let texts = self.aggiorna_palestra();
        self.open(Handler::Palestra, 89, texts, vec![]);
    }

    fn aggiorna_palestra(&mut self) -> Vec<(i32, String)> {
        let s = &self.state;
        let mut texts = vec![
            (104, mostra_soldi(s.soldi, s.euro)),
            (105, format!("{}/100", s.fama)),
        ];
        texts.push(if s.scad_pal_giorno < 1 {
            (106, "Nessun Abbonamento".to_string())
        } else {
            (
                106,
                format!(
                    "Scadenza abbonamento: {} {}",
                    s.scad_pal_giorno,
                    calendar::MESI[(s.scad_pal_mese - 1) as usize].0
                ),
            )
        });
        let ao = s.ao();
        let tan = match s.current_testa {
            1 => "Abbronzatura Lieve".to_string(),
            2 => "Abbronzatura Media".to_string(),
            3 => "Abbronzatura Pesante".to_string(),
            4 => format!("Carbonizzat{ao}..."),
            _ => format!("Non abbronzat{ao}"),
        };
        texts.push((107, tan));
        texts
    }

    fn on_palestra(&mut self, token: u64, id: i32) {
        match id {
            110 => {
                // Vai in palestra
                if self.state.scad_pal_giorno < 1 {
                    self.message(
                        "Palestra",
                        "Prima di poter venire in palestra devi fare un abbonamento !",
                    );
                } else {
                    self.bridge.play_sound(201);
                    if self.state.fama < 82 {
                        self.state.fama += 1;
                    }
                    self.eventi_palestra();
                }
                for (cid, text) in self.aggiorna_palestra() {
                    self.set_text(token, cid, text);
                }
            }
            111 => {
                // Lampada
                if PALESTRA_MEM[3] > self.state.soldi as i64 {
                    self.nomoney(89);
                } else {
                    if self.state.current_testa < 3 {
                        self.state.current_testa += 1;
                        if self.state.fama < 20 {
                            self.state.fama += 1;
                        }
                        if self.state.fama < 45 {
                            self.state.fama += 1;
                        }
                        if self.state.fama < 96 {
                            self.state.fama += 1;
                        }
                    } else {
                        self.state.current_testa = 4;
                        if self.state.fama > 8 {
                            self.state.fama -= 8;
                        }
                        if self.state.reputazione > 5 {
                            self.state.reputazione -= 5;
                        }
                        self.message(
                            "Lampada",
                            "L' eccessiva esposizione del tuo corpo ai raggi ultravioletti, provoca un avanzato grado di carbonizzazione e pure qualche piccola mutazione genetica...",
                        );
                    }
                    self.update_main(); // redraw the tabboz
                    self.bridge.play_sound(202);
                    self.state.soldi -= PALESTRA_MEM[3] as u64;
                }
                let i = self.rng.random(5 + self.state.fortuna);
                if i == 0 {
                    self.evento();
                }
                for (cid, text) in self.aggiorna_palestra() {
                    self.set_text(token, cid, text);
                }
            }
            115..=117 => {
                // Abbonamenti
                if self.state.scad_pal_giorno > 0 {
                    self.message(
                        "Palestra",
                        "Hai gia' un abbonamento, perche' te ne serve un altro ???",
                    );
                    return;
                }
                let idx = (id - 115) as usize;
                if PALESTRA_MEM[idx] > self.state.soldi as i64 {
                    self.nomoney(89);
                    return;
                }
                self.state.soldi -= PALESTRA_MEM[idx] as u64;
                let mesi = [1i32, 6, 12][idx];
                let mut mese = self.state.mese + mesi;
                if mese > 12 {
                    mese -= 12;
                }
                let mut giorno = self.state.giorno;
                let max = calendar::giorni_mese(mese);
                if giorno > max {
                    giorno = max;
                }
                self.state.scad_pal_mese = mese;
                self.state.scad_pal_giorno = giorno;
                for (cid, text) in self.aggiorna_palestra() {
                    self.set_text(token, cid, text);
                }
            }
            _ => self.close(token),
        }
    }

    /// EventiPalestra of vestiti.c.
    fn eventi_palestra(&mut self) {
        let i = self.rng.random(29 + self.state.fortuna / 2);
        if i > 9 {
            return;
        }
        let msg = tabboz_win::strings::find((1100 + i) as u16).unwrap_or("");
        self.message("Palestra...", msg);
        if self.state.reputazione > 10 {
            self.state.reputazione -= 4;
        }
    }

    // ---- compagnia --------------------------------------------------------

    fn open_compagnia(&mut self) {
        let texts = vec![(104, format!("{}/100", self.state.reputazione))];
        self.open(Handler::Compagnia, 6, texts, vec![]);
    }

    fn on_compagnia(&mut self, token: u64, id: i32) {
        match id {
            101 => {
                // Gareggia con lo scooter
                if self.state.scooter.stato <= 0 {
                    self.message(
                        "Gareggia con lo scooter",
                        "Con quale scooter vorresti gareggiare, visto che non lo possiedi ?",
                    );
                    return;
                }
                if self.state.scooter.attivita != 1 {
                    let att = N_ATTIVITA
                        .get(self.state.scooter.attivita as usize)
                        .copied()
                        .unwrap_or("mancante");
                    self.message(
                        "Gareggia con lo scooter",
                        &format!(
                            "Purtroppo non pui gareggiare visto che il tuo scooter e' {att}."
                        ),
                    );
                    return;
                }
                self.bridge.play_sound(701);
                let i = (1 + self.rng.random(6)) as usize;
                let nome = SCOOTER_MEM[i].1;
                let yes = self.confirm(
                    "Gareggia con lo scooter",
                    &format!("Accetti la sfida con un tabbozzo che ha un {nome} ?"),
                );
                if self.state.scooter.stato > 30 {
                    self.state.scooter.stato -= self.rng.random(2);
                }
                if yes {
                    let rival = SCOOTER_MEM[i].0 + 80 + self.rng.random(40);
                    let mio =
                        self.state.scooter.speed + self.state.scooter.stato + self.state.fortuna;
                    if rival > mio {
                        if self.state.reputazione > 80 {
                            self.state.reputazione -= 3;
                        }
                        if self.state.reputazione > 10 {
                            self.state.reputazione -= 2;
                        }
                        self.message(
                            "Hai perso",
                            "Dopo pochi metri si vede l' inferiorita' del tuo scooter...",
                        );
                    } else {
                        self.state.reputazione += 10;
                        if self.state.reputazione > 100 {
                            self.state.reputazione = 100;
                        }
                        self.message(
                            "Hai vinto",
                            "Con il tuo scooter, bruci l' avversario in partenza...",
                        );
                    }
                } else {
                    if self.state.reputazione > 80 {
                        self.state.reputazione -= 3;
                    }
                    if self.state.reputazione > 10 {
                        self.state.reputazione -= 2;
                    }
                }
                self.state.benzina -= 5;
                if self.state.benzina < 1 {
                    self.state.benzina = 0;
                }
                self.evento();
                self.set_text(token, 104, format!("{}/100", self.state.reputazione));
            }
            102 => {
                // Esci con la compagnia
                if self.state.reputazione < 57 {
                    self.state.reputazione += 1;
                }
                if self.state.reputazione < 37 {
                    self.state.reputazione += 1;
                }
                if self.state.reputazione < 12 {
                    self.state.reputazione += 1;
                }
                self.evento();
                self.close(token);
            }
            103 => {
                // Chiama la compagnia
                if self.state.reputazione < 16 {
                    self.message(
                        "Chiama la Compagnia",
                        "Con la scarsa reputazione che hai, tutti trovano qualcosa di meglio da fare piuttosto che venire.",
                    );
                    return;
                }
                if self.state.tempo_pestaggio > 0 {
                    if self.rng.random(2) == 1 {
                        self.message(
                            "Chiama la Compagnia",
                            "Dopo aver visto i tuoi amici, chi ti ha picchiato non si fara' piu' vedere in giro x un bel pezzo...",
                        );
                        if self.state.reputazione < 80 {
                            self.state.reputazione += 3;
                        }
                    } else {
                        self.message(
                            "Chiama la Compagnia",
                            "Anche i tuoi amici, al gran completo, vengono sacagnati di botte da chi ti aveva picchiato, accorgendosi cosi' che non sei solo tu ad essere una chiavica, ma lo sono anche loro...",
                        );
                        if self.state.reputazione < 95 {
                            self.state.reputazione += 5;
                        }
                    }
                    self.evento();
                } else {
                    self.message(
                        "Chiama la Compagnia (perche'?)",
                        "Visto che non c'e' nessuno da minacciare, tutti se ne vanno avviliti...",
                    );
                }
                self.set_text(token, 104, format!("{}/100", self.state.reputazione));
            }
            _ => self.close(token),
        }
    }

    // ---- about / personal info / spegnimi --------------------------------

    fn open_about(&mut self) {
        let version = format!("Tabboz Simulator {}", tabboz_core::VERSION);
        self.open(Handler::About, 2, vec![(115, version)], vec![]);
    }

    fn on_about(&mut self, token: u64, id: i32) {
        match id {
            113 => {
                // Norme di utilizzo (Atinom)
                self.message(
                    "Norme di utilizzo",
                    "Il biglietto e' valido solo dopo la convalida.Il biglietto deve essere conservato per tutta la durata del viaggio. Il diritto a viaggiare cessa al termine della tratta corrispondente al valore del biglietto. Il passeggero che al controllo non fosse in grado di presentare il biglietto o lo presentasse irriconoscibile, o comunque non valido, verra' abbattuto. La notifica del decesso verra' inviata ai parenti solo previo pagamento delle spese postali.",
                );
            }
            _ => self.close(token),
        }
    }

    fn open_personalinfo(&mut self) {
        let s = &self.state;
        let ao = s.ao();
        let texts = vec![
            (103, format!("{} {}", s.comp_giorno, calendar::MESI[(s.comp_mese - 1) as usize].0)),
            (
                104,
                (s.comp_giorno * 13 + s.comp_mese * 3 + 6070).to_string(),
            ),
            (
                108,
                if s.numeroditta < 1 {
                    if s.sesso == 'M' {
                        "Studente".to_string()
                    } else {
                        "Studentessa".to_string()
                    }
                } else {
                    format!("Sfruttat{ao}")
                },
            ),
            (109, format!("Nat{ao} il")),
            (
                110,
                if s.sesso == 'M' { "Celibe" } else { "Libera" }.to_string(),
            ),
            (111, s.residenza.clone()),
            (107, s.street.clone()),
            (105, s.city.clone()),
            (101, s.cognome.clone()),
            (102, s.nome.clone()),
        ];
        self.open(Handler::PersonalInfo, 11, texts, vec![]);
    }

    fn open_spegnimi(&mut self) {
        self.shutdown = 1;
        self.open(Handler::Spegnimi, 16, vec![], vec![101]);
    }

    fn on_spegnimi(&mut self, token: u64, id: i32) {
        match id {
            101 => self.shutdown = 1,
            102 => self.shutdown = 2,
            _ => {
                if self.shutdown == 1 {
                    crate::save::save(&self.state);
                    let _ = self.bridge.tx.send_event(UiRequest::Quit);
                }
                self.close(token);
            }
        }
    }
}

