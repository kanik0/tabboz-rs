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

use crate::ui::{AskKind, DialogAnswer, GameAction, MainSnapshot, TabbozSnapshot, UiBridge, UiRequest};

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
    Scooter,
    Concessionario,
    Showroom,
    VendiScooter,
    RiparaScooter,
    TruccaScooter,
    Pezzi,
    Lavoro,
    Interview,
    Quiz,
    InfoDitta,
    Tipa,
    CercaTipa,
    DueDiPicche,
    Cellular,
    CompraCellular,
    AbbonaCellular,
    Configuration,
    Format,
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

    // ---- scooter / lavoro / tipa / cellulare scratch state (the original
    // uses static locals; they live here per session) ----
    /// Showroom selection (ScooterMem index), 0 = none.
    scooter_scelta: usize,
    /// The showroom dialog closed with OK (buy) instead of Cancel.
    acquista_ok: bool,
    /// Vendi: offerta in migliaia di lire (static long offerta).
    sell_offerta: i64,
    /// Ripara: costo riparazione (static long costo).
    repair_costo: i64,
    /// Lavoro: ditta dell'intervista in corso (1..=8).
    interview_ditta: usize,
    /// Lavoro: l'intervista e' stata accettata (accetto == IDYES).
    interview_accettata: bool,
    /// Quiz: checkbox 101..=109 (Risposte1/2/3).
    risposte: [bool; 9],
    /// Cerca tipa: nome e figosita' della tipa in esame (nomeTemp/figTemp).
    nome_temp: String,
    fig_temp: i32,
    /// Due di picche: contatore click sul bottone 201.
    ddp_clicks: i32,
    /// Compra cellulare / abbonamento: radio selection.
    cellular_scelta: usize,
    abbonamento_scelta: usize,
    /// Format (reset): sesso scelto nel dialog 15.
    format_sesso: char,
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
            scooter_scelta: 0,
            acquista_ok: false,
            sell_offerta: 0,
            repair_costo: 0,
            interview_ditta: 0,
            interview_accettata: false,
            risposte: [false; 9],
            nome_temp: String::new(),
            fig_temp: 0,
            ddp_clicks: 0,
            cellular_scelta: 0,
            abbonamento_scelta: 0,
            format_sesso: 'M',
        }
    }

    /// Runs until the app quits.
    pub fn run(&mut self) {
        self.update_main();
        while let Ok(action) = self.rx.recv() {
            match action {
                GameAction::Button { token, id, x, y } => self.on_button(token, id, x, y),
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
        let handler = self.handler_of(token);
        self.frames.retain(|(t, _)| *t != token);
        let _ = self.bridge.tx.send_event(UiRequest::CloseFrame { token });
        // Post-close continuations: the code the original runs right after
        // DialogBox() returns (buy the scooter, evaluate the quiz, ...).
        if let Some(handler) = handler {
            self.continuation(handler);
        }
    }

    fn continuation(&mut self, handler: Handler) {
        match handler {
            Handler::Showroom => {
                // Concessionario case 101/102 after the showroom closes.
                if self.acquista_ok && self.scooter_scelta != 0 {
                    let scelta = self.scooter_scelta;
                    if self.state.scooter.stato != -1 {
                        self.message(
                            "Incentivi",
                            &format!(
                                "Per il tuo vecchio scooter da rottamare ti diamo {} di supervalutazione...",
                                mostra_soldi(1000, self.state.euro)
                            ),
                        );
                        self.state.soldi += 1000;
                        self.state.scooter = tabboz_core::state::Scooter::none();
                        self.state.benzina = 0;
                    }
                    let prezzo = SCOOTER_MEM_FULL[scelta].5 as u64;
                    if prezzo > self.state.soldi {
                        self.message(
                            "Non hai abbastanza soldi",
                            "Ti piacerebbe comprare lo scooter, vero ?\nPurtroppo, non hai abbastanza soldi...",
                        );
                        if self.state.reputazione > 3 {
                            self.state.reputazione -= 1;
                        }
                    } else {
                        self.state.soldi -= prezzo;
                        self.state.scooter = tabboz_core::state::Scooter {
                            speed: SCOOTER_MEM_FULL[scelta].0,
                            marmitta: SCOOTER_MEM_FULL[scelta].1,
                            carburatore: SCOOTER_MEM_FULL[scelta].2,
                            cc: SCOOTER_MEM_FULL[scelta].3,
                            filtro: SCOOTER_MEM_FULL[scelta].4,
                            prezzo: SCOOTER_MEM_FULL[scelta].5,
                            attivita: 1,
                            stato: 100,
                            nome: SCOOTER_MEM_FULL[scelta].6.to_string(),
                            fama: SCOOTER_MEM_FULL[scelta].7,
                        };
                        self.state.benzina = 20;
                        self.message(
                            "Lo scooter nuovo",
                            "Fai un giro del quartiere per farti vedere con lo scooter nuovo...",
                        );
                        self.state.reputazione += 4;
                        if self.state.reputazione > 100 {
                            self.state.reputazione = 100;
                        }
                    }
                    self.evento();
                }
                if let Some(t) = self.frame_of(Handler::Concessionario) {
                    let soldi = mostra_soldi(self.state.soldi, self.state.euro);
                    self.set_text(t, 104, soldi);
                }
            }
            Handler::Concessionario => {
                // Scooter case 101 after the concessionario closes.
                self.evento();
                if let Some(t) = self.frame_of(Handler::Scooter) {
                    self.refresh_scooter_frame(t);
                }
            }
            Handler::Interview => {
                // Lavoro case 110 after the interview dialog closes.
                if self.interview_accettata {
                    self.risposte = [false; 9];
                    let scheda = self.rng.random(9);
                    self.open(Handler::Quiz, 200 + scheda as u32, vec![], vec![]);
                } else {
                    self.message("Cerca Lavoro", "Allora sparisci...");
                }
            }
            Handler::Quiz => {
                // Lavoro case 110 after the quiz closes: the answers are
                // checked with the weights of the original (1/10/100).
                let mut rcheck = 0i32;
                for (i, on) in self.risposte.iter().enumerate() {
                    if *on {
                        rcheck += match i / 3 {
                            0 => 1,
                            1 => 10,
                            _ => 100,
                        };
                    }
                }
                if rcheck != 111 {
                    if self.state.sesso == 'M' {
                        self.message("Sei un po' stupido...", "Mi spieghi perche' dovremmo assumere qualcuno che non e' neanche in grado di mettere delle crocette su un foglio ???");
                    } else {
                        self.message("Sei un po' stupida...", "Signorina,mi spieghi perche' dovremmo assumere qualcuno che non e' neanche in grado di mettere delle crocette su un foglio ???");
                    }
                } else if (self.state.reputazione + self.state.fortuna + self.rng.random(80))
                    > self.rng.random(200)
                {
                    let n_ditta = self.interview_ditta;
                    self.state.impegno = 10 + self.rng.random(20);
                    self.state.giorni_di_lavoro = 1;
                    self.state.stipendio = 1000 + (self.rng.random(10) as i64) * 100;
                    self.state.numeroditta = n_ditta as i32;
                    let nome = LAVORO_MEM[n_ditta];
                    if self.state.sesso == 'M' {
                        self.message(
                            "Hai trovato lavoro !",
                            &format!("SEI STATO ASSUNTO ! Ora sei un felice dipendente della {nome} !"),
                        );
                    } else {
                        self.message(
                            "Hai trovato lavoro !",
                            &format!("SEI STATA ASSUNTA ! Ora sei una felice dipendente della {nome} !"),
                        );
                    }
                } else {
                    if self.state.sesso == 'M' {
                        self.message("Cerca Lavoro", "Mi dispiace ragazzo, ma non sei riuscito a superare il test... Ora puoi anche portare la tua brutta faccia fuori dal mio ufficio, prima che ti faccia buttare fuori a calci... Grazie e arrivederci...");
                    } else {
                        self.message("Cerca Lavoro", "Mi dispiace signorina, ma non e' riuscito a superare il test... Se ne vada immediatamente, grazie...");
                    }
                    if self.state.reputazione > 10 {
                        self.state.reputazione -= 2;
                    }
                }
                self.evento();
                if let Some(t) = self.frame_of(Handler::Lavoro) {
                    self.refresh_lavoro_frame(t);
                }
            }
            Handler::Format => {
                // Configuration case 203 -> FormatTabboz OK: ResetMe(0).
                let sesso = self.format_sesso;
                self.state = tabboz_core::GameState::reset(false, &mut self.rng);
                self.state.sesso = sesso;
                if sesso == 'M' {
                    self.state.nome = "Tizio".into();
                    self.state.cognome = "Caio".into();
                } else {
                    self.state.nome = "Nessuna".into();
                    self.state.cognome = "In Particolare".into();
                }
                self.update_main();
            }
            _ => {}
        }
    }

    /// Token of the last opened frame driven by this handler.
    fn frame_of(&self, handler: Handler) -> Option<u64> {
        self.frames.iter().rev().find(|(_, h)| *h == handler).map(|(t, _)| *t)
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

    // ---- event routing ----------------------------------------------------

    fn handler_of(&self, token: u64) -> Option<Handler> {
        self.frames.iter().find(|(t, _)| *t == token).map(|(_, h)| *h)
    }

    fn on_button(&mut self, token: u64, id: i32, x: i32, y: i32) {
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
            Some(Handler::Scooter) => self.on_scooter(token, id),
            Some(Handler::Concessionario) => self.on_concessionario(token, id),
            Some(Handler::Showroom) => self.on_showroom(token, id),
            Some(Handler::VendiScooter) => self.on_vendi_scooter(token, id),
            Some(Handler::RiparaScooter) => self.on_ripara_scooter(token, id),
            Some(Handler::TruccaScooter) => self.on_trucca_scooter(token, id),
            Some(Handler::Pezzi) => self.on_pezzi(token, id),
            Some(Handler::Lavoro) => self.on_lavoro(token, id),
            Some(Handler::Interview) => self.on_interview(token, id),
            Some(Handler::Quiz) => self.on_quiz(token, id),
            Some(Handler::InfoDitta) => self.close(token),
            Some(Handler::Tipa) => self.on_tipa(token, id, x, y),
            Some(Handler::CercaTipa) => self.on_cerca_tipa(token, id),
            Some(Handler::DueDiPicche) => self.on_due_di_picche(token, id),
            Some(Handler::Cellular) => self.on_cellular(token, id),
            Some(Handler::CompraCellular) => self.on_compra_cellular(token, id),
            Some(Handler::AbbonaCellular) => self.on_abbona_cellulare(token, id),
            Some(Handler::Configuration) => self.on_configuration(token, id),
            Some(Handler::Format) => self.on_format(token, id),
            _ => {}
        }
    }

    fn on_closed(&mut self, token: u64) {
        // The dialog close button (X) is IDCANCEL in the original: it only
        // ends the dialog, never the application (Spegnimi included).
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
            130 => self.open_scooter(),
            132 => self.open_disco(),
            133 => self.open_tipa(),
            134 => self.open_compagnia(),
            135 => self.open_famiglia(),
            136 => self.on_main_button(136),
            137 => self.open_lavoro(),
            139 => self.open_personalinfo(),
            140 => self.open_configuration(),
            141 => self.run_tabacchi(),
            142 => self.run_palestra(),
            143..=147 => self.run_vestiti(id),
            155 => self.open_cellular(),
            _ => {}
        }
    }

    fn on_main_button(&mut self, id: i32) {
        match id {
            103 => self.open_personalinfo(), // BMPView click
            120 => self.open_about(),
            130 => self.open_scooter(),
            131 => self.open_negozi(),
            132 => self.open_disco(),
            133 => self.open_tipa(),
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
            137 => self.open_lavoro(),
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
            112 => self.open_cellular(),
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
                let nome = SCOOTER_MEM_FULL[i].6;
                let yes = self.confirm(
                    "Gareggia con lo scooter",
                    &format!("Accetti la sfida con un tabbozzo che ha un {nome} ?"),
                );
                if self.state.scooter.stato > 30 {
                    self.state.scooter.stato -= self.rng.random(2);
                }
                if yes {
                    let rival = SCOOTER_MEM_FULL[i].0 + 80 + self.rng.random(40);
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
            // Help: the Progresso joke of the original, dialog stays open.
            110 => self.message(
                "Guida del Tabboz Simulator",
                "'Spegni il computer ed esci di casa'\n\nPubblicita' Progresso per il recupero dei giovani disadattati a causa dei computer sponsorizzata da Obscured Truckware.",
            ),
            // IDOK: boolean_shutdown != 0 closes the main dialog — the app
            // exits (ExitWindows of radio 102 is outside this port's scope).
            1 => {
                crate::save::save(&self.state);
                let _ = self.bridge.tx.send_event(UiRequest::Quit);
            }
            // IDCANCEL: back to the game, boolean_shutdown = 0.
            2 => {
                self.shutdown = 0;
                self.close(token);
            }
            _ => {}
        }
    }
}


// ===========================================================================
// M6: scooter, lavoro, tipa, telefonino, configuration (faithful ports of
// scooter.c, lavoro.c, tipa.c, telefono.c and the Configuration handler).
// ===========================================================================

/// ScooterMem of scooter.c: (speed, marmitta, carburatore, cc, filtro,
/// prezzo, nome, fama). Index 0 = nessuno scooter.
#[allow(clippy::type_complexity)] // the 1:1 shape of NEWSTSCOOTER
const SCOOTER_MEM_FULL: [(i32, i32, i32, i32, i32, i64, &str, i32); 8] = [
    (0, 0, 0, 0, 0, 0, "Nessuno scooter", 0),
    (65, 0, 0, 0, 0, 2498, "Magutty Firecow", 5),
    (75, 0, 1, 1, 1, 4348, "Honda F98", 10),
    (105, 1, 1, 2, 1, 6498, "Mizzubisci R200 Millenium", 15),
    (75, 0, 0, 1, 1, 4298, "Magutty Firecow+", 7),
    (100, 0, 1, 2, 1, 5998, "Magutty Firecow II", 10),
    (100, 0, 1, 2, 1, 6348, "Honda F98s", 13),
    (250, 0, 5, 5, 0, 1450, "Lexux LS400 ", 60),
];

/// PezziMem: prices of the tuning parts (marmitte, carburatori, cc, filtri).
const PEZZI_MEM: [i64; 15] = [
    400, 500, 600, // marmitte
    300, 470, 650, 800, // carburatori
    200, 400, 800, 1000, // cc
    50, 120, 270, 400, // filtri
];

/// tabella[] of scooter.c: speed by cc + carburatore*6.
const TABELLA: [i32; 36] = [
    65, 70, -100, -100, -100, -100, //
    70, 80, 95, -100, -100, -100, //
    -1000, 90, 100, 115, -100, -100, //
    -1000, -1000, 110, 125, 135, -100, //
    -1000, -1000, -1000, 130, 150, -100, //
    -1000, -1000, -1000, -1000, -1000, 250,
];

const N_MARMITTA: [&str; 4] = ["standard", "silenziosa", "rumorosa", "rumorosissima"];
const N_CARBURATORE: [&str; 6] = ["12/10", "16/16", "19/19", "20/20", "24/24", "custom"];
const N_CC: [&str; 6] = ["50cc", "70cc", "90cc", "120cc", "150cc", "3969cc"];
const N_FILTRO: [&str; 5] = ["standard", "P1", "P2", "P2+", "Extreme"];

/// LavoroMem of lavoro.c (nome only; speed = fuori porta).
/// (never checked in the original either, kept for fidelity)
#[allow(dead_code)]
const LAVORO_MEM_FUORI_PORTA: [bool; 9] = [false, false, false, true, false, true, true, true, true];
const LAVORO_MEM: [&str; 9] = [
    "---",
    "Magneti Budelli",
    "Diamine",
    "Testmec",
    "Ti Impalo Bene Bene",
    "October",
    "Arlond's",
    "286 - Computer d' annata",
    "Ricopio",
];

/// CellularMem of telefono.c: (dual, fama, stato, prezzo, nome).
const CELLULAR_MEM: [(i32, i32, i32, i64, &str); 3] = [
    (0, 2, 100, 290, "Motorolo d170"),
    (0, 7, 100, 590, "Motorolo 8700"),
    (1, 10, 100, 990, "Macro TAC 8900"),
];

/// AbbonamentMem of telefono.c: (abbonamento, dualonly, creditorest, fama,
/// prezzo, nome).
const ABBONAMENT_MEM: [(i32, i32, i32, i32, i64, &str); 9] = [
    (1, 0, 50, 1, 100, "Onmitel"), // Abbonamenti
    (1, 0, 50, 1, 100, "DIM"),
    (1, 1, 100, 1, 100, "Vind"),
    (0, 0, 50, 1, 60, "Onmitel"), // Ricariche
    (0, 0, 100, 1, 110, "Onmitel"),
    (0, 0, 50, 1, 60, "DIM"),
    (0, 0, 100, 1, 110, "DIM"),
    (0, 1, 50, 1, 50, "Vind"),
    (0, 1, 100, 1, 100, "Vind"),
];

impl Game {
    // ---- scooter ----------------------------------------------------------

    /// CalcolaVelocita of scooter.c (preview skips the state checks, like
    /// the original's showscooter flag).
    fn calcola_velocita(&mut self, preview: bool) {
        let s = &mut self.state.scooter;
        s.speed = (s.marmitta * 5) + (s.filtro * 5) + TABELLA[(s.cc + s.carburatore * 6) as usize];
        s.attivita = 1;
        if preview {
            return;
        }
        if s.speed <= -500 {
            s.attivita = 3; // invasato
        } else if s.speed <= -1 {
            s.attivita = 2; // ingrippato
        }
        if self.state.benzina < 1 {
            s.attivita = 6; // a secco
        }
        if s.attivita != 1 {
            let att = N_ATTIVITA[s.attivita as usize];
            let msg = format!("Il tuo scooter e' {att}.");
            self.message("Attenzione", &msg);
        }
    }

    /// AggiornaScooter of scooter.c.
    fn scooter_texts(&mut self) -> Vec<(i32, String)> {
        let mut texts = vec![(104, mostra_soldi(self.state.soldi, self.state.euro))];
        let s = &self.state.scooter;
        if s.stato != -1 {
            texts.push((107, format!("{}.{}l", self.state.benzina / 10, self.state.benzina % 10)));
            texts.push((110, self.mostra_speed()));
            texts.push((111, N_MARMITTA[s.marmitta as usize].to_string()));
            texts.push((112, N_CARBURATORE[s.carburatore as usize].to_string()));
            texts.push((113, N_CC[s.cc as usize].to_string()));
            texts.push((114, N_FILTRO[s.filtro as usize].to_string()));
            texts.push((115, format!("{}%", s.stato)));
            texts.push((116, s.nome.clone()));
            texts.push((117, mostra_soldi(s.prezzo as u64, self.state.euro)));
        } else {
            for id in [107, 110, 111, 112, 113, 114, 115, 116, 117] {
                texts.push((id, String::new()));
            }
        }
        texts
    }

    /// MostraSpeed of scooter.c.
    fn mostra_speed(&self) -> String {
        let s = &self.state.scooter;
        match s.attivita {
            1 => format!("{}Km/h", s.speed),
            2..=6 => format!("({})", N_ATTIVITA[s.attivita as usize]),
            _ => String::new(),
        }
    }

    fn refresh_scooter_frame(&mut self, token: u64) {
        for (cid, text) in self.scooter_texts() {
            self.set_text(token, cid, text);
        }
    }

    fn open_scooter(&mut self) {
        let mut texts = self.scooter_texts();
        let mut park_label = "Parcheggia scooter".to_string();
        if self.state.scooter.attivita == 4 {
            park_label = "Usa scooter".to_string();
        }
        texts.push((105, park_label));
        self.open(Handler::Scooter, 7, texts, vec![]);
    }

    fn on_scooter(&mut self, token: u64, id: i32) {
        let ao = self.state.ao();
        match id {
            101 => {
                // Acquista -> concessionario
                if self.state.vacanza == 2 {
                    self.message("Concessionario", &format!("Oh, tip{ao}... oggi il concessionario e' chiuso..."));
                    return;
                }
                let texts = vec![(104, mostra_soldi(self.state.soldi, self.state.euro))];
                self.open(Handler::Concessionario, 70, texts, vec![]);
            }
            102 => {
                // Trucca
                if self.state.scooter.stato != -1 {
                    if self.state.vacanza != 2 {
                        if self.state.sound_active {
                            self.bridge.play_sound(101);
                        }
                        let texts = self.scooter_texts();
                        self.open(Handler::TruccaScooter, 73, texts, vec![]);
                        return;
                    }
                    self.message("Trucca lo scooter", &format!("Oh, tip{ao}... oggi il meccanico e' chiuso..."));
                } else {
                    self.message(
                        "Trucca lo scooter",
                        "Scusa, ma quale scooter avresti intenzione di truccare visto che non ne hai neanche uno ???",
                    );
                }
                let mut park = "Parcheggia scooter".to_string();
                if self.state.scooter.attivita == 4 {
                    park = "Usa scooter".to_string();
                }
                self.set_text(token, 105, park);
                self.evento();
            }
            103 => {
                // Ripara
                if self.state.scooter.stato == -1 {
                    self.message(
                        "Ripara lo scooter",
                        "Mi spieghi come fai a farti riparare lo scooter se non lo hai ???",
                    );
                } else if self.state.scooter.stato == 100 {
                    self.message(
                        "Ripara lo scooter",
                        "Che motivi hai per voleer riparare il tuo scooter\nvisto che e' al 100% di efficienza ???",
                    );
                } else if self.state.vacanza == 2 {
                    self.message("Ripara lo scooter", &format!("Oh, tip{ao}... oggi il meccanico e' chiuso..."));
                } else {
                    let costo = (self.state.scooter.prezzo / 100 * (100 - self.state.scooter.stato as i64)) + 10;
                    self.repair_costo = costo;
                    let texts = vec![
                        (104, mostra_soldi(self.state.soldi, self.state.euro)),
                        (105, mostra_soldi(costo as u64, self.state.euro)),
                    ];
                    self.open(Handler::RiparaScooter, 72, texts, vec![]);
                }
            }
            105 => {
                // Parcheggia / Usa scooter
                if self.state.scooter.stato < 0 {
                    self.message(
                        "Parcheggia lo scooter",
                        "Mi spieghi come fai a parcheggiare lo scooter se non lo hai ???",
                    );
                    return;
                }
                match self.state.scooter.attivita {
                    1 => {
                        self.state.scooter.attivita = 4;
                        self.set_text(token, 105, "Usa scooter".to_string());
                    }
                    4 => {
                        self.state.scooter.attivita = 1;
                        self.set_text(token, 105, "Parcheggia scooter".to_string());
                    }
                    a => {
                        let msg = format!(
                            "Mi spieghi come fai a parcheggiare lo scooter visto che e' {} ???",
                            N_ATTIVITA[a as usize]
                        );
                        self.message("Parcheggia lo scooter", &msg);
                    }
                }
                self.refresh_scooter_frame(token);
            }
            106 => {
                // Fai benzina
                if self.state.scooter.stato < 0 {
                    self.message("Fai benza", "Mi spieghi come fai a far benzina allo scooter se non lo hai ???");
                    return;
                }
                match self.state.scooter.attivita {
                    1 | 2 | 3 | 6 => {
                        if self.state.soldi < 10 {
                            let msg = format!(
                                "Al distributore automatico puoi fare un minimo di {} di benzina...",
                                mostra_soldi(10, self.state.euro)
                            );
                            self.message("Fai benza", &msg);
                        } else {
                            self.state.soldi -= 10;
                            self.state.benzina = 50; // 5 litri
                            if self.state.scooter.cc == 5 {
                                self.state.benzina = 850; // la macchinina figa
                            }
                            self.calcola_velocita(false);
                            let msg = format!(
                                "Fai {} di benzina e riempi lo scooter...",
                                mostra_soldi(10, self.state.euro)
                            );
                            self.message("Fai benza", &msg);
                        }
                    }
                    a => {
                        let msg = format!(
                            "Mi spieghi come fai a far benzina allo scooter visto che e' {} ???",
                            N_ATTIVITA[a as usize]
                        );
                        self.message("Fai benza", &msg);
                    }
                }
                self.refresh_scooter_frame(token);
                self.evento();
            }
            _ => self.close(token),
        }
    }

    fn on_concessionario(&mut self, token: u64, id: i32) {
        match id {
            101 | 102 => {
                // Malagutty (78) / altre marche (79)
                let dlg = if id == 101 { 78 } else { 79 };
                self.scooter_scelta = 0;
                self.acquista_ok = false;
                self.open(Handler::Showroom, dlg, vec![], vec![]);
            }
            103 => {
                // Vendi
                if self.state.scooter.stato == -1 {
                    self.message(
                        "Vendi lo scooter",
                        "Scusa, ma quale scooter avresti intenzione di vendere visto che non ne hai neanche uno ???",
                    );
                } else {
                    let base = self.state.scooter.prezzo / 100;
                    let stato = self.state.scooter.stato;
                    let mut offerta = if self.state.scooter.attivita == 1 || self.state.scooter.attivita == 4 {
                        base * (stato as i64 - 10 - self.rng.random(10) as i64)
                    } else {
                        base * (stato as i64 - 50 - self.rng.random(10) as i64)
                    };
                    if offerta < 50 {
                        offerta = 50;
                    }
                    self.sell_offerta = offerta;
                    let mut texts = self.scooter_texts();
                    texts.push((118, mostra_soldi(offerta as u64, self.state.euro)));
                    self.open(Handler::VendiScooter, 71, texts, vec![]);
                }
                let soldi = mostra_soldi(self.state.soldi, self.state.euro);
                self.set_text(token, 104, soldi);
            }
            _ => self.close(token),
        }
    }

    fn on_showroom(&mut self, token: u64, id: i32) {
        match id {
            121..=126 => {
                // Preview of the selected scooter (the original shows the
                // computed speed, then resets the display).
                self.scooter_scelta = (id - 120) as usize;
            }
            1 => {
                self.acquista_ok = true;
                self.close(token);
            }
            _ => {
                self.acquista_ok = false;
                self.scooter_scelta = 0;
                self.close(token);
            }
        }
    }

    fn on_vendi_scooter(&mut self, token: u64, id: i32) {
        match id {
            1 => {
                self.state.scooter = tabboz_core::state::Scooter::none();
                self.state.benzina = 0;
                self.state.soldi += self.sell_offerta as u64;
                self.close(token);
                self.update_main();
            }
            _ => self.close(token),
        }
    }

    fn on_ripara_scooter(&mut self, token: u64, id: i32) {
        match id {
            1 => {
                if self.repair_costo > self.state.soldi as i64 {
                    self.nomoney(7); // SCOOTER
                } else {
                    self.state.scooter.stato = 100;
                    self.state.soldi -= self.repair_costo as u64;
                    self.calcola_velocita(false);
                }
                self.close(token);
                if let Some(t) = self.frame_of(Handler::Scooter) {
                    self.refresh_scooter_frame(t);
                }
                self.update_main();
            }
            _ => self.close(token),
        }
    }

    fn on_trucca_scooter(&mut self, token: u64, id: i32) {
        match id {
            121..=124 => {
                // Carburatore(74) / Marmitta(75) / Pistone(76) / Filtro(77)
                let dlg = (id - 121 + 74) as u32;
                let s = &self.state.scooter;
                let mut texts = vec![
                    (109, s.nome.clone()),
                    (105, N_CARBURATORE[s.carburatore as usize].to_string()),
                    (106, N_MARMITTA[s.marmitta as usize].to_string()),
                    (107, N_CC[s.cc as usize].to_string()),
                    (108, N_FILTRO[s.filtro as usize].to_string()),
                    (104, mostra_soldi(self.state.soldi, self.state.euro)),
                ];
                for i in 110..125 {
                    texts.push((i, mostra_soldi(PEZZI_MEM[(i - 110) as usize] as u64, self.state.euro)));
                }
                self.open(Handler::Pezzi, dlg, texts, vec![]);
            }
            _ => self.close(token),
        }
    }

    fn on_pezzi(&mut self, token: u64, id: i32) {
        match id {
            130..=144 => {
                let idx = (id - 130) as usize;
                if (PEZZI_MEM[idx] as u64) > self.state.soldi {
                    self.nomoney(7); // SCOOTER
                    return;
                }
                self.state.soldi -= PEZZI_MEM[idx] as u64;
                match id {
                    130..=132 => self.state.scooter.marmitta = id - 129,
                    133..=136 => self.state.scooter.carburatore = id - 132,
                    137..=140 => self.state.scooter.cc = id - 136,
                    _ => self.state.scooter.filtro = id - 140,
                }
                self.calcola_velocita(false);
                self.close(token);
                if let Some(t) = self.frame_of(Handler::TruccaScooter) {
                    for (cid, text) in self.scooter_texts() {
                        self.set_text(t, cid, text);
                    }
                }
                self.update_main();
            }
            _ => self.close(token),
        }
    }

    // ---- lavoro -----------------------------------------------------------

    fn open_lavoro(&mut self) {
        let mut texts = vec![];
        let leccaculo = if self.state.sesso == 'M' {
            "Fai il leccaculo"
        } else {
            "Fai la leccaculo"
        };
        texts.push((113, leccaculo.to_string()));
        self.open(Handler::Lavoro, 13, texts, vec![]);
        if let Some(t) = self.frame_of(Handler::Lavoro) {
            self.refresh_lavoro_frame(t);
        }
    }

    /// AggiornaLavoro of lavoro.c.
    fn refresh_lavoro_frame(&mut self, token: u64) {
        let mut texts = vec![(104, mostra_soldi(self.state.soldi, self.state.euro))];
        if self.state.numeroditta < 1 {
            texts.push((105, String::new()));
            texts.push((106, String::new()));
            texts.push((107, String::new()));
        } else {
            texts.push((107, format!("{}/100", self.state.impegno)));
            let nome = LAVORO_MEM[self.state.numeroditta as usize].to_string();
            texts.push((105, nome));
            texts.push((106, mostra_soldi(self.state.stipendio as u64, self.state.euro)));
        }
        for (cid, text) in texts {
            self.set_text(token, cid, text);
        }
    }

    /// GiornoDiLavoro: the two checks shared by every work action.
    fn giorno_di_lavoro(&mut self, s: &str) -> bool {
        let ao = self.state.ao();
        if self.state.numeroditta < 1 {
            let msg = format!("Forse non ti ricordi che sei disokkupat{ao}...");
            self.message(s, &msg);
            return true;
        }
        if self.state.vacanza == 2 {
            let msg = format!("Arrivat{ao} davanti ai cancelli della ditta li trovi inrimediabilmente chiusi...");
            self.message(s, &msg);
            return true;
        }
        false
    }

    fn on_lavoro(&mut self, token: u64, id: i32) {
        let ao = self.state.ao();
        match id {
            110 => {
                // Cerca lavoro
                if self.state.vacanza == 2 {
                    let msg = format!("Arrivat{ao} davanti ai cancelli della ditta li trovi inrimediabilmente chiusi...");
                    self.message("Cerca Lavoro", &msg);
                    return;
                }
                if self.state.numeroditta > 0 {
                    self.message("Cerca Lavoro", "Forse non ti ricordi che hai gia' un lavoro...");
                    return;
                }
                let n_ditta = self.rng.random(8) + 1; // 1..=8
                self.interview_ditta = n_ditta as usize;
                self.interview_accettata = false;
                self.open(Handler::Interview, 389 + n_ditta as u32, vec![], vec![]);
            }
            111 => {
                // Licenziati
                if self.giorno_di_lavoro("Licenziati") {
                    return;
                }
                let nome = LAVORO_MEM[self.state.numeroditta as usize];
                let msg = format!("Sei proprio sicur{ao} di voler dare le dimissioni dalla {nome} ?");
                if self.confirm("Licenziati", &msg) {
                    self.state.numeroditta = 0;
                    self.state.impegno = 0;
                    self.state.giorni_di_lavoro = 0;
                    self.state.stipendio = 0;
                    self.evento();
                }
                self.refresh_lavoro_frame(token);
            }
            112 => {
                // Chiedi aumento salario
                if self.giorno_di_lavoro("Chiedi aumento salario") {
                    return;
                }
                if self.state.impegno > 90 {
                    if (30 + self.state.fortuna) > (30 + self.rng.random(50)) {
                        self.message("Chiedi aumento salario", "Forse per questa volta potremmo darti qualcosina in piu'...");
                        self.state.stipendio += ((self.rng.random(1) + 1) as i64) * 100;
                        self.state.impegno -= 30;
                        self.evento();
                    } else {
                        if self.state.sesso == 'M' {
                            self.message("Chiedi aumento salario", "Vedi di scordartelo,bastardo...");
                        } else {
                            self.message("Chiedi aumento salario", "Vedi di scordartelo,stronzetta...");
                        }
                        self.state.impegno -= 20;
                        self.evento();
                    }
                } else {
                    self.message("Chiedi aumento salario", "Che cosa vorresti ??? SCORDATELO !!!!");
                }
                self.refresh_lavoro_frame(token);
            }
            113 => {
                // Fai il leccaculo
                if self.giorno_di_lavoro("Fai il leccaculo") {
                    return;
                }
                if self.state.sound_active {
                    self.bridge.play_sound(503);
                }
                if self.state.reputazione > 20 {
                    self.state.reputazione -= 1;
                }
                if self.state.impegno < 99 {
                    self.state.impegno += 1;
                }
                let i = self.rng.random(self.state.fortuna + 3);
                if i == 0 {
                    self.evento();
                }
                self.refresh_lavoro_frame(token);
            }
            114 => {
                // Elenco ditte / info della propria ditta
                if self.state.numeroditta == 0 {
                    self.open(Handler::InfoDitta, 210, vec![], vec![]);
                } else {
                    let dlg = (self.state.numeroditta + 289) as u32;
                    self.open(Handler::InfoDitta, dlg, vec![], vec![]);
                }
            }
            115 => {
                // Sciopera
                if self.giorno_di_lavoro("Sciopera") {
                    return;
                }
                if self.state.sound_active {
                    self.bridge.play_sound(502);
                }
                if self.state.reputazione < 85 {
                    self.state.reputazione += 10;
                }
                if self.state.impegno > 19 {
                    self.state.impegno -= 15;
                }
                let i = self.rng.random(self.state.fortuna + 3);
                if i == 0 {
                    self.evento();
                }
                self.evento();
                self.refresh_lavoro_frame(token);
            }
            116 => {
                // Lavora
                if self.giorno_di_lavoro("Lavora") {
                    return;
                }
                if self.state.impegno < 85 {
                    self.state.impegno += 1;
                }
                if self.state.sound_active {
                    self.bridge.play_sound(501);
                }
                self.evento();
                self.refresh_lavoro_frame(token);
            }
            _ => self.close(token),
        }
    }

    fn on_interview(&mut self, token: u64, id: i32) {
        match id {
            1 => {
                self.interview_accettata = true;
                self.close(token);
            }
            _ => {
                self.interview_accettata = false;
                self.close(token);
            }
        }
    }

    fn on_quiz(&mut self, token: u64, id: i32) {
        match id {
            101..=109 => {
                let i = (id - 101) as usize;
                self.risposte[i] = !self.risposte[i];
            }
            _ => self.close(token),
        }
    }

    // ---- tipa -------------------------------------------------------------

    /// AggiornaTipa of tipa.c (including the original label bug: the
    /// "Cerca Tipo" SetDlgItemText is unconditional, so the label always
    /// ends up "Cerca Tipo" for male players too).
    fn refresh_tipa_frame(&mut self, token: u64) {
        let mut texts: Vec<(i32, String)> = Vec::new();
        if self.state.rapporti == 0 {
            // Original bug kept for fidelity: this call overwrites the
            // "Cerca Tipa" label set just above it.
            texts.push((101, "Cerca Tipo".to_string()));
            texts.push((105, String::new()));
            texts.push((106, String::new()));
            texts.push((107, String::new()));
        } else {
            if self.state.sesso == 'M' {
                texts.push((101, "Cerca Nuova Tipa".to_string()));
            } else {
                texts.push((101, "Cerca Nuovo Tipo".to_string()));
            }
            texts.push((105, self.state.nometipa.clone()));
            texts.push((106, self.state.fig_tipa.to_string()));
            texts.push((107, format!("{}/100", self.state.rapporti)));
        }
        texts.push((104, format!("{}/100", self.state.fama)));
        for (cid, text) in texts {
            self.set_text(token, cid, text);
        }
    }

    fn open_tipa(&mut self) {
        self.open(Handler::Tipa, 9, vec![], vec![]);
        if let Some(t) = self.frame_of(Handler::Tipa) {
            self.refresh_tipa_frame(t);
        }
    }

    fn on_tipa(&mut self, token: u64, id: i32, x: i32, y: i32) {
        let _ao = self.state.ao();
        match id {
            110 => {
                // Cerca tipa: dialog 91 (M) / 191 (F)
                let dlg = if self.state.sesso == 'M' { 91 } else { 191 };
                self.open_cerca_tipa(dlg);
            }
            111 => {
                // Lascia tipa
                if self.state.rapporti <= 0 {
                    if self.state.sesso == 'M' {
                        self.message("Lascia Tipa", "Scusa, che ragazza avresti intenzione di lasciare ???");
                    } else {
                        self.message("Lascia Tipo", "Scusa, che tipo avresti intenzione di lasciare, visto che sei sola come un cane ???");
                    }
                    return;
                }
                let msg = format!("Sei proprio sicuro di voler lasciare {} ?", self.state.nometipa);
                let title = if self.state.sesso == 'M' { "Lascia tipa" } else { "Molla tipo" };
                if self.confirm(title, &msg) {
                    if self.state.sound_active {
                        self.bridge.play_sound(603);
                    }
                    self.state.rapporti = 0;
                    if self.state.fig_tipa >= 79 && self.state.sesso == 'M' {
                        self.message(
                            "Idiota...",
                            "Appena vengono a sapere quello che hai fatto, i tuoi amici ti prendono a scarpate.\nQualcuno, piu' furbo di te, va a consolarla...",
                        );
                        self.state.reputazione -= 8;
                        if self.state.reputazione < 0 {
                            self.state.reputazione = 0;
                        }
                    }
                    if self.state.fig_tipa <= 40 && self.state.sesso == 'M' {
                        self.state.reputazione += 4;
                        if self.state.reputazione > 100 {
                            self.state.reputazione = 100;
                        }
                    }
                    self.evento();
                }
                self.refresh_tipa_frame(token);
            }
            112 => {
                // Telefona
                if self.state.rapporti <= 0 {
                    if self.state.sesso == 'M' {
                        self.message("Non sei molto intelligente...", "Scusa, che ragazza vorresti chiamare ???");
                    } else {
                        self.message("Non sei molto intelligente...", "Scusa, che ragazzo vorresti chiamare, visto che sei sola ???");
                    }
                    return;
                }
                if self.state.soldi <= 5
                    && (self.state.abbonamento.creditorest < 2 && self.state.cellular.stato < 0)
                {
                    self.message(
                        "Non toccare quel telefono...",
                        "Sei fai ancora una telefonata, ti spezzo le gambe, disse tuo padre con un accetta in mano...",
                    );
                } else {
                    if self.state.sound_active {
                        self.bridge.play_sound(602);
                    }
                    if self.state.abbonamento.creditorest >= 2 && self.state.cellular.stato > -1 {
                        self.state.abbonamento.creditorest -= 2;
                    } else {
                        self.state.soldi -= 5;
                    }
                    if self.state.rapporti <= 60 {
                        self.state.rapporti += 1;
                    }
                }
                self.refresh_tipa_frame(token);
            }
            113 => {
                // Esci con la tipa
                if self.state.rapporti <= 0 {
                    if self.state.sesso == 'M' {
                        self.message("Non sei molto intelligente...", "Scusa, con che tipa vorresti uscire ???");
                    } else {
                        self.message("Non sei molto intelligente...", "Scusa, ma con chi vorresti uscire, ???");
                    }
                    return;
                }
                if self.state.sesso == 'M' {
                    if self.state.scooter.stato <= 0 {
                        self.message(
                            "Compra lo scooter",
                            "Finche' non comprerai lo scooter, non usciremo piu' insieme...",
                        );
                        return;
                    }
                    if self.state.scooter.attivita != 1 {
                        let att = N_ATTIVITA[self.state.scooter.attivita as usize];
                        let msg = format!("Finche' il tuo scooter restera' {att} non potremo uscire insieme...");
                        self.message("Risistema la scooter", &msg);
                        return;
                    }
                    if self.state.scooter.stato <= 35 {
                        self.message(
                            "Ripara lo scooter",
                            "Finche' non riparerai lo scooter, non usciremo piu' insieme...",
                        );
                        return;
                    }
                }
                if self.state.soldi < 15 {
                    if self.state.sesso == 'M' {
                        self.message(
                            "Sei povero",
                            "Se mi vuoi portare fuori, cerca di avere almeno un po' di soldi...",
                        );
                    } else {
                        self.message("Che palle", "Oh tipa... cioe' non posso pagare sempre tutto io, cioe' ohhhh...");
                    }
                    return;
                }
                self.state.soldi -= 15;
                self.state.rapporti += 5;
                if self.state.rapporti > 100 {
                    self.state.rapporti = 100;
                }
                if self.state.fig_tipa > self.state.fama {
                    self.state.fama += 1;
                    if self.state.fama > 100 {
                        self.state.fama = 100;
                    }
                }
                self.state.benzina -= 3;
                if self.state.benzina < 1 {
                    self.state.benzina = 0;
                }
                self.calcola_velocita(false);
                self.refresh_tipa_frame(token);
                self.update_main();
            }
            130 => {
                // BMPTipa "palpatina" (coordinate check of tabimg.c).
                let in_tette = (138..=170).contains(&x) && (50..=65).contains(&y);
                let in_culo = (104..=136).contains(&x) && (78..=166).contains(&y);
                if in_tette || in_culo {
                    let r = self.state.rapporti;
                    let fig = self.state.fig_tipa;
                    if r > 0 && r < 20 + fig / 2 {
                        if self.state.sound_active {
                            self.bridge.play_sound(604);
                        }
                        self.message("Palpatina...", "Brutto porco, che cazzo tocchi ?");
                        if self.state.rapporti > 5 {
                            self.state.rapporti -= 3;
                        }
                        if let Some(t) = self.frame_of(Handler::Tipa) {
                            self.refresh_tipa_frame(t);
                        }
                    } else if r > 0 && r < 30 + fig / 2 {
                        self.message("Palpatina...", "Dai, smettila... Voi uomini pensato solo a quello...");
                    } else if r >= 30 + fig / 2 {
                        self.message("Palpatina...", "Mmhhhhhhhh.........");
                    }
                }
            }
            _ => self.close(token),
        }
    }

    fn open_cerca_tipa(&mut self, dlg: u32) {
        let fig_temp = self.rng.random(71) + 30;
        self.fig_temp = fig_temp;
        let (desc, nome_id) = if self.state.sesso == 'M' {
            (descrizione_tipa(fig_temp), 200 + self.rng.random(20))
        } else {
            (descrizione_tipo(fig_temp), 1200 + self.rng.random(20))
        };
        self.nome_temp = tabboz_win::strings::find(nome_id as u16).unwrap_or("").to_string();
        let texts = vec![
            (107, desc.to_string()),
            (105, self.nome_temp.clone()),
            (106, format!("{fig_temp}/100")),
        ];
        self.open(Handler::CercaTipa, dlg, texts, vec![]);
    }

    fn on_cerca_tipa(&mut self, token: u64, id: i32) {
        match id {
            101 => {
                // Ci provo...
                let fig_temp = self.fig_temp;
                let success =
                    (fig_temp * 2 + self.rng.random(50)) <= (self.state.fama + self.state.reputazione + self.rng.random(30));
                if success {
                    if self.state.sesso == 'M' {
                        self.message(
                            "E' andata bene !",
                            "Con il tuo fascino nascosto da tabbozzo, seduci la tipa e ti ci metti insieme.",
                        );
                    } else {
                        self.message("Qualcono ti caga...", "Ora non ti puoi piu' lamentare di essere sola...");
                    }
                    if self.state.rapporti > 0 {
                        // Gia' impegnato: DueDonne (92 M / 192 F).
                        let dlg = if self.state.sesso == 'M' { 92 } else { 192 };
                        let resta = format!("Resto con {}", self.state.nometipa);
                        let nuova = if self.state.nometipa == self.nome_temp {
                            if self.state.sesso == 'M' {
                                "Preferisco quella nuova".to_string()
                            } else {
                                "Preferisco quello nuovo".to_string()
                            }
                        } else {
                            format!("Preferisco {}", self.nome_temp)
                        };
                        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                        let _ = self.bridge.tx.send_event(UiRequest::Ask {
                            kind: AskKind::Dialog {
                                id: dlg,
                                texts: vec![(2, resta), (102, nuova)],
                            },
                            reply: reply_tx,
                        });
                        let answer = reply_rx.recv().unwrap_or(DialogAnswer::Ok);
                        match answer {
                            DialogAnswer::Ok => {} // resta con la vecchia
                            DialogAnswer::Yes => {
                                // 101: la vita e' bella...
                                if self.state.sesso == 'M' {
                                    let msg = format!(
                                        "Mentre sei appartato con la {}, arriva la tua ragazza, {}, ti tira uno schiaffo e ti lascia.Capendo finalmente di che pasta sei fatto, anche la {} si allontana...",
                                        self.nome_temp, self.state.nometipa, self.nome_temp
                                    );
                                    self.message("La vita e' bella", &msg);
                                } else {
                                    let msg = format!(
                                        "{} viene a sapere che di {}, gli spacca la faccia e ti molla...Dopo questa tragica esperienza anche {} sparisce...",
                                        self.state.nometipa, self.nome_temp, self.nome_temp
                                    );
                                    self.message("La vita e' bella", &msg);
                                }
                                self.state.rapporti = 0;
                                self.state.reputazione -= 8;
                                if self.state.reputazione < 0 {
                                    self.state.reputazione = 0;
                                }
                                self.state.fama -= 4;
                                if self.state.fama < 0 {
                                    self.state.fama = 0;
                                }
                            }
                            DialogAnswer::No => {
                                // 102: preferisci quella nuova...
                                self.state.nometipa = self.nome_temp.clone();
                                self.state.fig_tipa = fig_temp;
                                self.state.rapporti = 30 + self.rng.random(15);
                                self.state.fama += fig_temp / 10;
                                if self.state.fama > 100 {
                                    self.state.fama = 100;
                                }
                                self.state.reputazione += fig_temp / 13;
                                if self.state.reputazione > 100 {
                                    self.state.reputazione = 100;
                                }
                            }
                        }
                    } else {
                        self.state.nometipa = self.nome_temp.clone();
                        self.state.fig_tipa = fig_temp;
                        self.state.rapporti = 30 + self.rng.random(15);
                        self.state.fama += fig_temp / 10;
                        if self.state.fama > 100 {
                            self.state.fama = 100;
                        }
                        self.state.reputazione += fig_temp / 13;
                        if self.state.reputazione > 100 {
                            self.state.reputazione = 100;
                        }
                    }
                } else {
                    // 2 di picche...
                    if self.state.sound_active {
                        self.bridge.play_sound(601);
                    }
                    self.state.ddp += 1;
                    self.state.reputazione -= 2;
                    if self.state.reputazione < 0 {
                        self.state.reputazione = 0;
                    }
                    self.state.fama -= 2;
                    if self.state.fama < 0 {
                        self.state.fama = 0;
                    }
                    let i = if self.state.sesso == 'M' {
                        300 + self.rng.random(20)
                    } else {
                        1300 + self.rng.random(20)
                    };
                    let sfiga = tabboz_win::strings::find(i as u16).unwrap_or("").to_string();
                    self.ddp_clicks = 0;
                    self.open(Handler::DueDiPicche, 95, vec![(105, sfiga)], vec![]);
                }
                self.evento();
                self.close(token);
                if let Some(t) = self.frame_of(Handler::Tipa) {
                    self.refresh_tipa_frame(t);
                }
            }
            _ => self.close(token),
        }
    }

    fn on_due_di_picche(&mut self, token: u64, id: i32) {
        match id {
            201 => {
                self.ddp_clicks += 1;
                if self.ddp_clicks > 5 {
                    let msg = format!(
                        "Fino ad ora hai preso {} due di picche !\nNon ti preoccupare, capita a tutti di prendere qualche due di picche nella vita ...",
                        self.state.ddp
                    );
                    self.message("La vita e' bella...", &msg);
                    self.ddp_clicks = 0;
                }
            }
            _ => self.close(token),
        }
    }

    // ---- telefonino -------------------------------------------------------

    /// AggiornaCell of telefono.c.
    fn refresh_cellular_frame(&mut self, token: u64) {
        let mut texts = vec![(104, mostra_soldi(self.state.soldi, self.state.euro))];
        if self.state.cellular.stato > -1 {
            texts.push((120, self.state.cellular.nome.clone()));
        } else {
            texts.push((120, String::new()));
        }
        if self.state.abbonamento.creditorest > -1 {
            texts.push((121, self.state.abbonamento.nome.clone()));
            texts.push((
                122,
                mostra_soldi(self.state.abbonamento.creditorest as u64, self.state.euro),
            ));
        } else {
            texts.push((121, String::new()));
            texts.push((122, String::new()));
        }
        for (cid, text) in texts {
            self.set_text(token, cid, text);
        }
    }

    fn cellular_vacanza(&mut self) -> bool {
        if self.state.vacanza == 2 {
            self.message(
                "Telefonino",
                "Stranamente, in un giorno di vacanza, il negozio e' chiuso...",
            );
            true
        } else {
            false
        }
    }

    fn open_cellular(&mut self) {
        self.open(Handler::Cellular, 120, vec![], vec![]);
        if let Some(t) = self.frame_of(Handler::Cellular) {
            self.refresh_cellular_frame(t);
        }
    }

    fn on_cellular(&mut self, token: u64, id: i32) {
        match id {
            110 => {
                // Compra
                if !self.cellular_vacanza() {
                    self.cellular_scelta = 0;
                    let mut texts = vec![(104, mostra_soldi(self.state.soldi, self.state.euro))];
                    for (i, cell) in CELLULAR_MEM.iter().enumerate() {
                        texts.push((
                            120 + i as i32,
                            mostra_soldi(cell.3 as u64, self.state.euro),
                        ));
                    }
                    self.open(Handler::CompraCellular, 121, texts, vec![]);
                }
            }
            111 => {
                // Vendi
                if !self.cellular_vacanza() {
                    if self.state.cellular.stato > -1 {
                        let offerta = self.state.cellular.prezzo / 2 + 15;
                        let msg = format!(
                            "Ti posso dare {} per il tuo telefonino... vuoi vendermelo ?",
                            mostra_soldi(offerta as u64, self.state.euro)
                        );
                        if self.confirm("Telefonino", &msg) {
                            self.state.cellular.stato = -1;
                            self.state.soldi += offerta as u64;
                        } else {
                            self.message("Telefonino", "Allora vai a farti fottere, pirletta !");
                        }
                    } else {
                        self.message("Telefonino", "Che telefonino vuoi vendere, pirletta ?");
                    }
                    self.refresh_cellular_frame(token);
                }
            }
            112 => {
                // Abbonamento
                if !self.cellular_vacanza() {
                    self.abbonamento_scelta = 0;
                    let mut texts = vec![(104, mostra_soldi(self.state.soldi, self.state.euro))];
                    if self.state.abbonamento.creditorest > -1 {
                        texts.push((105, self.state.abbonamento.nome.clone()));
                    }
                    for (i, abbonamento) in ABBONAMENT_MEM.iter().enumerate() {
                        texts.push((
                            110 + i as i32,
                            mostra_soldi(abbonamento.4 as u64, self.state.euro),
                        ));
                    }
                    self.open(Handler::AbbonaCellular, 123, texts, vec![]);
                }
            }
            _ => self.close(token),
        }
    }

    fn on_compra_cellular(&mut self, token: u64, id: i32) {
        match id {
            110..=112 => {
                self.cellular_scelta = (id - 110) as usize;
            }
            1 => {
                let sel = self.cellular_scelta;
                let (dual, fama, _, prezzo, nome) = CELLULAR_MEM[sel];
                if (prezzo as u64) > self.state.soldi {
                    self.nomoney(123); // CELLULRABBONAM
                } else {
                    self.state.soldi -= prezzo as u64;
                    self.state.cellular = tabboz_core::state::Cellular {
                        dual,
                        fama,
                        stato: 100,
                        prezzo,
                        nome: nome.to_string(),
                    };
                    self.state.fama += fama;
                    if self.state.fama > 100 {
                        self.state.fama = 100;
                    }
                }
                self.close(token);
                if let Some(t) = self.frame_of(Handler::Cellular) {
                    self.refresh_cellular_frame(t);
                }
                self.update_main();
            }
            _ => self.close(token),
        }
    }

    fn on_abbona_cellulare(&mut self, token: u64, id: i32) {
        match id {
            110..=118 => {
                self.abbonamento_scelta = (id - 110) as usize;
            }
            1 => {
                let sel = self.abbonamento_scelta;
                let (abbonamento, _dualonly, creditorest, _fama, prezzo, nome) = ABBONAMENT_MEM[sel];
                if (prezzo as u64) > self.state.soldi {
                    self.nomoney(123);
                } else if abbonamento == 1 {
                    // Abbonamento, no problem...
                    self.state.soldi -= prezzo as u64;
                    self.state.abbonamento = tabboz_core::state::Abbonamento {
                        abbonamento,
                        dualonly: _dualonly,
                        creditorest,
                        fama: _fama,
                        prezzo,
                        nome: nome.to_string(),
                    };
                    if self.state.sound_active && self.state.cellular.stato > -1 {
                        self.bridge.play_sound(602);
                    }
                } else if self.state.abbonamento.creditorest > -1
                    && self.state.abbonamento.nome == nome
                {
                    // Ricarica della stessa SIM...
                    self.state.soldi -= prezzo as u64;
                    self.state.abbonamento.creditorest += creditorest;
                    if self.state.sound_active && self.state.cellular.stato > -1 {
                        self.bridge.play_sound(602);
                    }
                } else {
                    self.message(
                        "Telefonino",
                        "Oh, che  te ne fai di una ricarica se non hai la sim ???",
                    );
                }
                self.close(token);
                if let Some(t) = self.frame_of(Handler::Cellular) {
                    self.refresh_cellular_frame(t);
                }
            }
            _ => self.close(token),
        }
    }

    // ---- configuration ----------------------------------------------------

    fn open_configuration(&mut self) {
        // Radios of the difficulty level (fortuna) and the toggles.
        let mut radios = vec![];
        let fortuna = self.state.fortuna;
        let radio = if fortuna >= 20 {
            101
        } else if fortuna >= 15 {
            102
        } else if fortuna >= 10 {
            103
        } else if fortuna >= 5 {
            104
        } else {
            105
        };
        radios.push(radio);
        let mut checks = vec![];
        if self.state.euro {
            checks.push(107);
        }
        if self.state.sound_active {
            checks.push(110);
        }
        let t = self.token(Handler::Configuration);
        let _ = self.bridge.tx.send_event(UiRequest::ShowFrame {
            token: t,
            dlg_id: 14,
            texts: vec![],
            radios,
            checks,
        });
    }

    fn on_configuration(&mut self, token: u64, id: i32) {
        match id {
            101 => self.state.fortuna = 20,
            102 => self.state.fortuna = 15,
            103 => self.state.fortuna = 10,
            104 => self.state.fortuna = 5,
            105 => self.state.fortuna = 0,
            107 => {
                self.state.euro = !self.state.euro;
            }
            110 => {
                self.state.sound_active = !self.state.sound_active;
            }
            // Startup Logo / Enable Timer: the original toggles settings the
            // port does not model; the click only flips the visual box.
            106 | 108 => {}
            203 => {
                // Reset: dialog 15 (scelta del sesso).
                self.format_sesso = if self.rng.random(2) == 1 { 'M' } else { 'F' };
                self.open(Handler::Format, 15, vec![], vec![102]);
            }
            _ => {
                crate::save::save(&self.state); // SalvaTutto on close (EM)
                self.close(token);
            }
        }
    }

    fn on_format(&mut self, token: u64, id: i32) {
        match id {
            100 => self.format_sesso = 'M',
            101 => self.format_sesso = 'F',
            102 => {
                self.format_sesso = if self.rng.random(2) == 1 { 'M' } else { 'F' }
            }
            _ => self.close(token),
        }
    }
}

/// DescrizioneTipa of tipa.c.
fn descrizione_tipa(f: i32) -> &'static str {
    if f > 97 {
        "Ultramegafiga"
    } else if f > 90 {
        "Fighissima"
    } else if f > 83 {
        "Molto figa"
    } else if f > 72 {
        "Figa"
    } else if f > 67 {
        "Abbastanza Figa"
    } else if f > 55 {
        "Interessante"
    } else if f > 45 {
        "Passabile"
    } else if f > 35 {
        "Puo' piacere.."
    } else {
        "E' un tipo..."
    }
}

/// DescrizioneTipo of tipa.c.
fn descrizione_tipo(f: i32) -> &'static str {
    if f > 97 {
        "Ultramegafigo"
    } else if f > 90 {
        "Bellissimo"
    } else if f > 83 {
        "Molto figo"
    } else if f > 72 {
        "Bello"
    } else if f > 67 {
        "Abbastanza Figo"
    } else if f > 55 {
        "Interessante"
    } else if f > 45 {
        "Passabile"
    } else if f > 35 {
        "Puo' piacere.."
    } else {
        "Inutile..."
    }
}
