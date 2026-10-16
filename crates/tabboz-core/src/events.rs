// Event engine: faithful port of eventi.c Evento().
//
// Evento() is the "time passes" function of the game: it is called after
// almost every player action. It advances the day (Giorno), applies the
// daily upkeep (cigarettes, phone credit, relationships, work, allowance)
// and then rolls one random event.
//
// All message boxes of the original go through the Interact trait, so the
// logic stays headless and testable; the UI (milestone 4) implements it.

use crate::calendar;
use crate::rng::Rng;
use crate::state::GameState;

/// What the game needs to show to the player during Evento().
pub trait Interact {
    /// MB_OK message box.
    fn message(&mut self, _title: &str, _text: &str) {}
    /// MB_YESNO question; true = IDYES.
    fn confirm(&mut self, _title: &str, _text: &str) -> bool {
        false
    }
    /// Modal dialog with only an OK button (MostraMetallone and friends):
    /// dialog id plus (control id -> text) overrides.
    fn dialog(&mut self, _id: u32, _set_text: &[(i32, String)]) {}
    /// Modal dialog with multiple buttons (DueDonne): returns the clicked
    /// button id (101 nuova, 102 resta, 1/0 = default/close = resta).
    fn choice(&mut self, _id: u32, _set_text: &[(i32, String)]) -> i32 {
        1
    }
    /// TabbozPlaySound, only called when sound is active.
    fn play_sound(&mut self, _id: u16) {}
}

/// Interact implementation that records nothing (used by tests that only
/// verify state transitions).
pub struct Silent;

impl Interact for Silent {}

/// Evento(): one full "game turn". Mutates the state exactly like the
/// original C and drives the UI through `ui`.
pub fn evento(state: &mut GameState, rng: &mut Rng, ui: &mut dyn Interact) {
    // Anti-cheat, exactly like the original (clamped to 0..=100).
    state.fortuna = state.fortuna.clamp(0, 100);

    // ---- Giorno --------------------------------------------------------------
    let rep = calendar::giorno(state);
    for (title, text) in &rep.messages {
        ui.message(title, text);
    }
    if let Some(p) = &rep.pagella {
        if !p.ammesso {
            play_sound(ui, state, 401); // bocciato
        }
        // MostraPagella fills dialog 110: control 119 = ammesso/a,
        // controls 120..128 = marks, control 129 = condotta.
        let mut set: Vec<(i32, String)> = Vec::new();
        set.push((119, p.testo_ammesso(state.ao())));
        for (i, v) in p.voti.iter().enumerate() {
            set.push((120 + i as i32, v.to_string()));
        }
        set.push((129, p.condotta.to_string()));
        ui.dialog(110, &set);
        if p.ammesso {
            state.soldi += 200;
        }
    }

    if state.tempo_pestaggio > 0 {
        state.tempo_pestaggio -= 1;
    }

    // ---- Sigarette -------------------------------------------------------------
    if state.sizze > 0 {
        state.sizze -= 1;
        if state.sizze == 0 {
            ui.message(
                "Sei senza sigarette !",
                "Apri il tuo pacchetto di sigarette e lo trovi disperatamente vuoto...",
            );
            if state.reputazione > 10 {
                state.reputazione -= 3;
            }
        } else if state.sizze < 3 {
            ui.message("Sigarette...", "Ti accorgi che stai per finire le tue sizze.");
        }
    }

    // ---- Cellulare --------------------------------------------------------------
    if state.abbonamento.creditorest > 0 && state.cellular.stato > -1 {
        state.abbonamento.creditorest -= 1;
        if state.fama < 55 {
            state.fama += 1;
        }
        if state.abbonamento.creditorest == 0 {
            ui.message(
                "Telefonino",
                "Cerchi di telefonare e ti accorgi di aver finito i soldi a tua disposizione...",
            );
        } else if state.abbonamento.creditorest < 3 {
            ui.message(
                "Telefonino",
                "Ti accorgi che stai per finire la ricarica del tuo telefonino.",
            );
        }
    }

    if state.cellular.stato == 1 {
        state.cellular.stato = -1;
        ui.message(
            "Telefonino",
            "Dopo una vita di duro lavoro, a furia di prendere botte, il tuo cellulare si spacca...",
        );
    }

    // ---- Rapporti tipa ------------------------------------------------------------
    if state.rapporti > 3 {
        let i = rng.random(5) - 3;
        if i > 0 {
            state.rapporti -= 1;
        }
    }

    if state.rapporti > 0 && state.rapporti < 98 {
        let i = rng.random((state.rapporti + state.fortuna + state.fama) * 3 + 1) + 1;
        if i < 11 {
            // da 1 a 10: la donna ti molla...
            play_sound(ui, state, 603);
            state.rapporti = 0;
            state.fig_tipa = 0;
            let title = if state.sesso == 'M' {
                "La tipa ti molla..."
            } else {
                "Vieni mollata..."
            };
            let id = if state.sesso == 'M' { 1040 + i } else { 1340 + i };
            ui.message(title, &crate::state::string(id as u16));
            state.reputazione -= 11 - i;
            if state.reputazione < 0 {
                state.reputazione = 0;
            }
        }
    }

    // ---- Lavoro -----------------------------------------------------------------
    if state.impegno > 3 {
        let i = rng.random(7) - 3;
        if i > 0 {
            state.impegno -= 1;
        }
    }

    if state.numeroditta > 0 {
        let i = rng.random(state.impegno * 2 + state.fortuna * 3);
        if i < 2 {
            state.impegno = 0;
            state.giorni_di_lavoro = 0;
            state.stipendio = 0;
            state.numeroditta = 0;
            play_sound(ui, state, 504);
            let ao = state.ao();
            ui.message(
                "Perdi il lavoro...",
                &format!("Un bel giorno ti svegli e scopri di essere stat{ao} licenziat{ao}."),
            );
        }
    }

    // ---- Paghetta (Sabato) ---------------------------------------------------------
    if state.giornoset == 6 {
        if state.studio >= 45 {
            state.soldi += state.paghetta;
            if state.studio >= 80 {
                play_sound(ui, state, 1100);
                state.soldi += state.paghetta;
                ui.message(
                    "Paghetta settimanale",
                    "Visto che vai bene a scuola, ti diamo il doppio della paghetta...",
                );
            }
        } else {
            play_sound(ui, state, 1200);
            state.soldi += state.paghetta / 2;
            ui.message(
                "Paghetta settimanale",
                "Finche' non andrai bene a scuola, ti daremo solo meta' della paghetta...",
            );
        }
    }

    // ---- Evento casuale ----------------------------------------------------------
    let caso = rng.random(100 + state.fortuna * 2);

    if caso < 51 {
        match caso {
            // ---- Metalloni e Manovali ----
            1..=10 => {
                if state.sesso == 'F' {
                    return; // le tiponne non vengono pestate
                }
                state.reputazione -= caso;
                if state.reputazione < 0 {
                    state.reputazione = 0;
                }
                // Dialog 100-105 with a random street name in control 111
                // (MostraMetallone).
                let via_id = rng.random(50) + 450;
                let mut via = crate::state::string(via_id as u16);
                if let Some(first) = via.get_mut(0..1) {
                    let lower: String = first.to_lowercase();
                    via.replace_range(0..1, &lower);
                }
                play_sound(ui, state, 1400);
                let dlg = 100 + rng.random(6);
                ui.dialog(dlg as u32, &[(111, via)]);
                state.tempo_pestaggio = 5;
            }

            // ---- Scooter ----
            11..=20 => {
                if state.scooter.stato != -1 && state.scooter.attivita == 1 {
                    if state.cellular.stato > -1 {
                        state.cellular.stato -= rng.random(8);
                        if state.cellular.stato < 0 {
                            state.cellular.stato = 0;
                        }
                    }
                    let dlg = if caso < 17 {
                        state.scooter.stato -= 35;
                        106 // camionista
                    } else {
                        state.scooter.stato -= 20;
                        107 // muro
                    };
                    ui.dialog(dlg, &[]);
                    state.reputazione -= 2;
                    if state.reputazione < 0 {
                        state.reputazione = 0;
                    }
                    if state.scooter.stato <= 0 {
                        ui.message(
                            "Scooter Distrutto",
                            "Quando ti rialzi ti accorgi che il tuo scooter e' ormai ridotto ad un ammasso di rottami.",
                        );
                        state.scooter = crate::state::Scooter::none();
                    }
                }
            }

            // ---- Figosita' ----
            // (the C fall-through subtracts progressively: 21-23 -> -5,
            //  24-25 -> -1, 26-27 -> -1, then -2 for every case)
            21..=30 => {
                let extra = match caso {
                    21..=23 => 5,
                    24..=25 => 1,
                    26..=27 => 1,
                    _ => 0,
                };
                state.fama -= extra;
                let ao = state.ao();
                ui.message(
                    &format!("Sei fortunat{ao}..."),
                    &crate::state::string((1000 + caso) as u16),
                );
                state.fama -= 2;
                if state.fama < 0 {
                    state.fama = 0;
                }
            }

            // ---- Scuola ----
            31..=40 => {
                if state.vacanza == 0 {
                    let i = (rng.random(9) + 1) as usize;
                    let mut msg = crate::state::string((1000 + caso) as u16);
                    msg.push_str(crate::state::MATERIE[i]);
                    ui.message("Scuola...", &msg);
                    if state.materie[i] >= 2 {
                        state.materie[i] -= 2;
                    }
                    state.calcola_studio();
                }
            }

            // ---- Tipa/o ci prova ----
            41..=42 => {
                if state.fama < 35 {
                    return; // figosita' < 35: nessuna speranza
                }
                let fig_temp = rng.random(state.fama - 30) + 30;
                let (nome_id, chi) = if state.sesso == 'M' {
                    (200 + rng.random(20), "tipa")
                } else {
                    (1200 + rng.random(20), "tipo")
                };
                let nome_temp = crate::state::string(nome_id as u16);
                let question = format!(
                    "Una {chi}, di nome {nome_temp} (Figosita' {fig_temp}/100), ci prova con te'...\nCi stai ???"
                );
                if !ui.confirm("Qualcuno ti caga...", &question) {
                    if fig_temp >= 79 && state.rapporti < 1 && state.sesso == 'M' {
                        ui.message(
                            "Idiota...",
                            "Appena vengono a sapere che non ti vuoi mettere insieme ad una figona come quella, i tuoi amici ti prendono a scarpate.",
                        );
                        state.reputazione -= 4;
                        if state.reputazione < 0 {
                            state.reputazione = 0;
                        }
                    }
                    return;
                }
                if state.rapporti > 0 {
                    // gia' impegnato: dialog DueDonne (92 M / 192 F)
                    let dlg = if state.sesso == 'M' { 92 } else { 192 };
                    let resta = format!("Resto con {}", state.nometipa);
                    let nuova = if state.nometipa == nome_temp {
                        if state.sesso == 'M' {
                            "Preferisco quella nuova".to_string()
                        } else {
                            "Preferisco quello nuovo".to_string()
                        }
                    } else {
                        format!("Preferisco {nome_temp}")
                    };
                    let answer = ui.choice(dlg, &[(2, resta), (102, nuova)]);
                    if answer == 101 {
                        // Ottima scelta... (tipa.c DueDonne case 101)
                        if state.sesso == 'M' {
                            ui.message(
                                "La vita e' bella",
                                &format!(
                                    "Mentre sei appartato con la {nome_temp}, arriva la tua ragazza, {}, ti tira uno schiaffo e ti lascia.Capendo finalmente di che pasta sei fatto, anche la {nome_temp} si allontana...",
                                    state.nometipa
                                ),
                            );
                        } else {
                            ui.message(
                                "La vita e' bella",
                                &format!(
                                    "{} viene a sapere che di {nome_temp}, gli spacca la faccia e ti molla...Dopo questa tragica esperienza anche {nome_temp} sparisce...",
                                    state.nometipa
                                ),
                            );
                        }
                        state.rapporti = 0;
                        state.reputazione -= 8;
                        if state.reputazione < 0 {
                            state.reputazione = 0;
                        }
                        state.fama -= 4;
                        if state.fama < 0 {
                            state.fama = 0;
                        }
                    } else if answer == 102 {
                        // Preferisci quella nuova...
                        state.nometipa = nome_temp.clone();
                        state.fig_tipa = fig_temp;
                        state.rapporti = 30 + rng.random(15);
                        state.fama += fig_temp / 10;
                        if state.fama > 100 {
                            state.fama = 100;
                        }
                        state.reputazione += fig_temp / 13;
                        if state.reputazione > 100 {
                            state.reputazione = 100;
                        }
                    }
                } else {
                    state.nometipa = nome_temp.clone();
                    state.fig_tipa = fig_temp;
                    state.rapporti = 45 + rng.random(15);
                    state.fama += fig_temp / 10;
                    if state.fama > 100 {
                        state.fama = 100;
                    }
                    state.reputazione += fig_temp / 13;
                    if state.reputazione > 100 {
                        state.reputazione = 100;
                    }
                }
            }

            // ---- Domande inutili della tipa ----
            43..=44 => {
                if state.rapporti > 0 && state.sesso == 'M' {
                    if caso == 43 {
                        if !ui.confirm("Domande inutili della Tipa...", "Mi ami ???") {
                            ui.message(
                                "Risposta sbagliata...",
                                "Sei sempre il solito stronzo.. non capisco perche' resto ancora con uno come cosi'...",
                            );
                            state.rapporti -= 45;
                            if state.rapporti < 5 {
                                state.rapporti = 5;
                            }
                        }
                    } else if ui.confirm("Domande inutili della Tipa...", "Ma sono ingrassata ???") {
                        ui.message(
                            "Risposta sbagliata...",
                            "Sei un bastardo, non capisci mai i miei problemi...",
                        );
                        state.rapporti -= 20;
                        if state.rapporti < 5 {
                            state.rapporti = 5;
                        }
                    }
                }
            }

            45..=48 => {} // "da fare" anche nell'originale

            // ---- Telefonino cade ----
            49..=50 => {
                if state.cellular.stato > -1 {
                    state.cellular.stato -= rng.random(8);
                    if state.cellular.stato < 0 {
                        state.cellular.stato = 0;
                    }
                    ui.message(
                        "Telefonino",
                        "Il telefonino di cade di tasca e vola per terra...",
                    );
                }
            }

            _ => {}
        }
    }
}

/// TabbozPlaySound guarded by sound_active, exactly like the original.
fn play_sound(ui: &mut dyn Interact, state: &GameState, id: u16) {
    if state.sound_active {
        ui.play_sound(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Interact that records everything and answers scripted confirms.
    type Recorded = (String, String);
    type DialogSet = Vec<(i32, String)>;

    struct Recorder {
        messages: RefCell<Vec<Recorded>>,
        dialogs: RefCell<Vec<(u32, DialogSet)>>,
        sounds: RefCell<Vec<u16>>,
        confirms: RefCell<Vec<Recorded>>,
    }

    impl Recorder {
        fn always_yes() -> Self {
            Recorder {
                messages: RefCell::new(vec![]),
                dialogs: RefCell::new(vec![]),
                sounds: RefCell::new(vec![]),
                confirms: RefCell::new(vec![]),
            }
        }
    }

    impl Interact for Recorder {
        fn message(&mut self, title: &str, text: &str) {
            self.messages.borrow_mut().push((title.into(), text.into()));
        }
        fn confirm(&mut self, title: &str, text: &str) -> bool {
            self.confirms.borrow_mut().push((title.into(), text.into()));
            // "Mi ami ???" must be answered NO to trigger the punishment;
            // every other question is answered YES.
            text != "Mi ami ???"
        }
        fn dialog(&mut self, id: u32, set: &[(i32, String)]) {
            self.dialogs.borrow_mut().push((id, set.to_vec()));
        }
        fn play_sound(&mut self, id: u16) {
            self.sounds.borrow_mut().push(id);
        }
    }

    #[test]
    fn evento_advances_the_day_and_keeps_invariants() {
        // Long soak: 200 days per seed across several seeds; the state must
        // stay within the original clamps and nothing may panic.
        for seed in 1..=20u32 {
            let mut s = GameState::reset(true, &mut Rng::with_seed(seed));
            let mut ui = Silent;
            for _ in 0..200 {
                evento(&mut s, &mut Rng::with_seed(seed.wrapping_mul(31)), &mut ui);
                assert!((0..=100).contains(&s.reputazione), "reputazione out of range");
                assert!((0..=100).contains(&s.fama), "fama out of range");
                assert!((1..=7).contains(&s.giornoset));
                assert!((1..=12).contains(&s.mese));
                assert!(s.giorno >= 1 && s.giorno <= 31);
            }
        }
    }

    #[test]
    fn cigarettes_run_out_and_cost_reputation() {
        let mut ui = Recorder::always_yes();
        let mut s = GameState::reset(true, &mut Rng::with_seed(99));
        s.sizze = 1;
        s.reputazione = 20;
        evento(&mut s, &mut Rng::with_seed(99), &mut ui);
        assert_eq!(s.sizze, 0);
        assert_eq!(s.reputazione, 17); // -3 when the packet empties
        assert!(ui
            .messages
            .borrow()
            .iter()
            .any(|(t, _)| t == "Sei senza sigarette !"));
    }

    #[test]
    fn phone_credit_decays_and_warns() {
        let mut ui = Recorder::always_yes();
        let mut s = GameState::reset(true, &mut Rng::with_seed(98));
        s.cellular.stato = 5;
        s.abbonamento.creditorest = 3;
        s.fama = 30;
        evento(&mut s, &mut Rng::with_seed(98), &mut ui);
        assert_eq!(s.abbonamento.creditorest, 2);
        assert_eq!(s.fama, 31); // < 55 grows while the phone works
        assert!(ui.messages.borrow().iter().any(|(t, _)| t == "Telefonino"));
    }

    #[test]
    fn dying_phone_dies_exactly_like_the_original() {
        let mut ui = Recorder::always_yes();
        let mut s = GameState::reset(true, &mut Rng::with_seed(97));
        s.cellular.stato = 1;
        evento(&mut s, &mut Rng::with_seed(97), &mut ui);
        assert_eq!(s.cellular.stato, -1);
        assert!(ui.messages.borrow().iter().any(|(t, _)| t == "Telefonino"));
    }

    #[test]
    fn saturday_allowance_follows_the_marks() {
        // Studio >= 80 -> double allowance; reset gives studio == 0 -> half.
        let mut ui = Recorder::always_yes();
        let mut s = GameState::reset(true, &mut Rng::with_seed(4));
        s.giornoset = 5; // tomorrow is Saturday
        s.studio = 80;
        evento(&mut s, &mut Rng::with_seed(4), &mut ui);
        assert_eq!(s.giornoset, 6);
        assert_eq!(s.soldi, 10 + 30 + 30); // paghetta x2

        let mut s = GameState::reset(true, &mut Rng::with_seed(4));
        s.giornoset = 5;
        s.studio = 0;
        evento(&mut s, &mut Rng::with_seed(4), &mut ui);
        assert_eq!(s.soldi, 10 + 15); // half allowance
    }

    #[test]
    fn random_events_fire_in_the_expected_range() {
        // Soak across seeds: dialogs 100..107 (pestaggi/scooter) must appear.
        let mut pestaggi = 0;
        let mut fama_events = 0;
        for seed in 1..=500u32 {
            let mut ui = Recorder::always_yes();
            let mut s = GameState::reset(true, &mut Rng::with_seed(seed));
            evento(&mut s, &mut Rng::with_seed(seed), &mut ui);
            let dialogs = ui.dialogs.borrow();
            if dialogs.iter().any(|(id, _)| (100..=107).contains(id)) {
                pestaggi += 1;
            }
            drop(dialogs);
            if ui.messages.borrow().iter().any(|(t, _)| t.starts_with("Sei fortunat")) {
                fama_events += 1;
            }
        }
        assert!(pestaggi > 20, "expected many pestaggi, got {pestaggi}");
        assert!(fama_events > 20, "expected many fama events, got {fama_events}");
    }

    #[test]
    fn scooter_accident_can_destroy_the_scooter() {
        // Drive the state into a fragile scooter and soak for a destruction.
        let mut destroyed = false;
        for seed in 1..=2000u32 {
            let mut ui = Recorder::always_yes();
            let mut s = GameState::reset(true, &mut Rng::with_seed(seed));
            s.scooter.stato = 35; // one camionista hit destroys it
            s.scooter.attivita = 1;
            evento(&mut s, &mut Rng::with_seed(seed), &mut ui);
            if s.scooter.stato == -1 {
                assert!(ui
                    .messages
                    .borrow()
                    .iter()
                    .any(|(t, _)| t == "Scooter Distrutto"));
                destroyed = true;
                break;
            }
        }
        assert!(destroyed, "scooter never destroyed across 2000 seeds");
    }

    #[test]
    fn tipa_hits_on_you_and_you_get_together() {
        // High fama so case 41/42 can fire; say yes and check the pairing.
        for seed in 1..=3000u32 {
            let mut ui = Recorder::always_yes();
            let mut s = GameState::reset(true, &mut Rng::with_seed(seed));
            s.fama = 50;
            s.rapporti = 0;
            evento(&mut s, &mut Rng::with_seed(seed), &mut ui);
            if !s.nometipa.is_empty() && s.rapporti > 0 {
                assert!((45..60).contains(&s.rapporti));
                assert!(s.fig_tipa >= 30);
                return;
            }
        }
        panic!("no successful pairing across 3000 seeds");
    }

    #[test]
    fn useless_questions_punish_wrong_answers() {
        for seed in 1..=3000u32 {
            let mut ui = Recorder::always_yes();
            let mut s = GameState::reset(true, &mut Rng::with_seed(seed));
            s.sesso = 'M';
            s.rapporti = 50;
            evento(&mut s, &mut Rng::with_seed(seed), &mut ui);
            let asked = ui
                .confirms
                .borrow()
                .iter()
                .any(|(t, _)| t == "Domande inutili della Tipa...");
            if asked {
                // "Mi ama ???" answered NO -> -45 (clamped to 5);
                // "Ma sono ingrassata ???" answered YES -> -20 (30, maybe -1 decay).
                if s.rapporti == 5 || s.rapporti >= 29 {
                    return;
                }
                panic!("unexpected rapporti after useless question: {}", s.rapporti);
            }
        }
        panic!("useless question never fired");
    }

    #[test]
    fn school_event_lowers_a_mark() {
        for seed in 1..=3000u32 {
            let mut ui = Recorder::always_yes();
            let mut s = GameState::reset(true, &mut Rng::with_seed(seed));
            s.vacanza = 1; // no: vacation suppresses school events...
            s.materie = [0, 8, 8, 8, 8, 8, 8, 8, 8, 8];
            // ...so run with vacation == 0
            s.vacanza = 0;
            let before = s.materie;
            evento(&mut s, &mut Rng::with_seed(seed), &mut ui);
            if ui.messages.borrow().iter().any(|(t, _)| t == "Scuola...") {
                let diff = before.iter().zip(s.materie.iter()).filter(|(a, b)| a != b).count();
                assert_eq!(diff, 1);
                let lowered = before
                    .iter()
                    .zip(s.materie.iter())
                    .find(|(a, b)| a != b)
                    .map(|(a, b)| a - b)
                    .unwrap();
                assert_eq!(lowered, 2);
                return;
            }
        }
        panic!("school event never fired");
    }

    #[test]
    fn metallone_event_sets_pestaggio_cooldown() {
        for seed in 1..=3000u32 {
            let mut ui = Recorder::always_yes();
            let mut s = GameState::reset(true, &mut Rng::with_seed(seed));
            s.sesso = 'M';
            evento(&mut s, &mut Rng::with_seed(seed), &mut ui);
            let dialogs = ui.dialogs.borrow();
            if let Some((id, set)) = dialogs.iter().find(|(id, _)| (100..=105).contains(id)) {
                assert!(set.iter().any(|(cid, _)| *cid == 111), "via missing in dialog {id}");
                drop(dialogs);
                assert_eq!(s.tempo_pestaggio, 5);
                assert!(s.reputazione >= 0);
                return;
            }
        }
        panic!("metallone event never fired");
    }
}
