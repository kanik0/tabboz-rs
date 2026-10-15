// Calendar engine: faithful port of tempo.c (Giorno, MostraPagella).

use crate::state::GameState;

/// STMESI InfoMese: month name and days.
pub const MESI: [(&str, i32); 12] = [
    ("Gennaio", 31),
    ("Febbraio", 28),
    ("Marzo", 31),
    ("Aprile", 30),
    ("Maggio", 31),
    ("Giugno", 30),
    ("Luglio", 31),
    ("Agosto", 31),
    ("Settembre", 30),
    ("Ottobre", 31),
    ("Novembre", 30),
    ("Dicembre", 31),
];

/// InfoSettimana (index 0..6, x_giornoset is 1-based; 7 = Domenica).
pub const SETTIMANA: [&str; 7] = [
    "Lunedi'",
    "Martedi'",
    "Mercoledi'",
    "Giovedi'",
    "Venerdi'",
    "Sabato",
    "Domenica",
];

/// STVACANZE InfoVacanze: (nome, giorno, mese, descrizione).
pub const VACANZE: [(&str, i32, i32, &str); 10] = [
    ("Capodanno", 1, 1, "Oggi e' capodanno !"),
    ("Epifania", 6, 1, "Epifania..."),
    ("Anniversario Liberazione", 25, 4, "Oggi mi sento liberato"),
    ("Festa dei lavoratori", 1, 5, "Nonostante nella tua vita, tu non faccia nulla, oggi fai festa anche tu..."),
    ("Ferragosto", 15, 8, "Oggi e' ferragosto..."),
    ("Tutti i Santi", 1, 11, "Figata, oggi e' vacanza..."),
    ("Sant' Ambrogio", 7, 12, "Visto che siamo a Milano, oggi facciamo festa."),
    ("Immacolata Concezione", 8, 12, "Oggi e' festa..."),
    ("Natale", 25, 12, "Buon Natale !!!"),
    ("Santo Stefano", 26, 12, "Buon Santo Stefano..."),
];

/// Days of a month (the original never accounts for leap February here).
pub fn giorni_mese(mese: i32) -> i32 {
    MESI[(mese - 1) as usize].1
}

/// What Giorno() has to show to the player (message boxes, pagella).
#[derive(Clone, Debug, Default)]
pub struct GiornoReport {
    /// (title, text) message boxes.
    pub messages: Vec<(String, String)>,
    /// Pagella of June 22nd, if it is that day.
    pub pagella: Option<Pagella>,
}

/// The school report (MostraPagella of tempo.c).
#[derive(Clone, Debug)]
pub struct Pagella {
    /// Marks of the 9 subjects (materie 1..=9).
    pub voti: [i32; 9],
    /// Condotta: 8 if fama > 75, else 9.
    pub condotta: i32,
    /// "ammesso/a" or "NON ammesso/a".
    pub ammesso: bool,
}

impl Pagella {
    pub fn testo_ammesso(&self, ao: char) -> String {
        if self.ammesso {
            format!("ammess{ao}")
        } else {
            format!("NON ammess{ao}")
        }
    }
}

/// Giorno(): advances the calendar by one day and produces the report.
/// State mutations match tempo.c line by line.
pub fn giorno(state: &mut GameState) -> GiornoReport {
    let mut rep = GiornoReport::default();

    state.giorno += 1;
    if state.giorno > giorni_mese(state.mese) {
        if state.mese == 2 && state.anno_bisesto == 1 && state.giorno == 29 {
            // "Anno bisesto, anno funesto..." (the original keeps the day on 29)
            rep.messages.push((
                "Anno Bisestile".to_string(),
                "Anno bisesto, anno funesto...".to_string(),
            ));
        } else {
            state.giorno = 1;
            state.mese += 1;
        }
    }

    if state.mese > 12 {
        state.mese = 1;
        // The original does `x_anno_bisesto = +1` (unary plus!), so the flag
        // becomes 1 every year; the "> 3" check never fires.
        state.anno_bisesto = 1;
        if state.anno_bisesto > 3 {
            state.anno_bisesto = 0;
        }
    }

    state.giornoset += 1;
    if state.giornoset > 7 {
        state.giornoset = 1;
        if state.current_testa > 0 {
            state.current_testa -= 1; // the tan fades every 7 days
        }
    }

    // ---- Stipendio ----------------------------------------------------------
    if state.impegno > 0 {
        state.giorni_di_lavoro += 1;
        if state.giorno == 27 && state.giorni_di_lavoro > 3 {
            let stipendietto: i64 = if state.giorni_di_lavoro > 29 {
                state.stipendio
            } else {
                state.stipendio * state.giorni_di_lavoro as i64 / 30
            };
            state.giorni_di_lavoro = 1;
            let ao = state.ao();
            let un_una = state.un_una();
            rep.messages.push((
                "Stipendio !".to_string(),
                format!(
                    "Visto che sei stat{ao} {un_una} brav{ao} dipendente sottomess{ao}, ora ti arriva il tuo misero stipendio di {}",
                    crate::money::mostra_soldi(stipendietto as u64, state.euro)
                ),
            ));
            state.soldi += stipendietto as u64;
        }
    }

    // ---- Palestra -----------------------------------------------------------
    if state.scad_pal_mese == state.mese && state.scad_pal_giorno == state.giorno {
        rep.messages.push((
            "Palestra".to_string(),
            "E' appena scaduto il tuo abbonamento della palestra...".to_string(),
        ));
        state.scad_pal_giorno = 0;
        state.scad_pal_mese = 0;
    }

    state.vacanza = 0;
    state.current_tipa = 0;

    // ---- Month-specific rules ------------------------------------------------
    match state.mese {
        1 => {
            if state.giorno < 7 {
                state.vacanza = 1; // Vacanze di Natale
                if state.rapporti > 0 {
                    state.current_tipa = 1; // tipa vestita da Babbo Natale
                }
            }
        }
        6 => {
            if state.giorno == 15 {
                rep.messages.push((
                    "Ultimo giorno di scuola".to_string(),
                    "Da domani iniziano le vacanza estive !".to_string(),
                ));
            }
            if state.giorno == 22 {
                rep.pagella = Some(pagella(state));
            }
            if state.giorno > 15 {
                state.vacanza = 1;
            }
        }
        7 | 8 => {
            state.vacanza = 1;
            if state.rapporti > 93 && state.fig_tipa > 95 {
                state.current_tipa = 2; // tipa al mare
            }
        }
        9 => {
            if state.giorno < 15 {
                state.vacanza = 1;
            }
            if state.giorno == 15 {
                rep.messages.push((
                    "Primo giorno di scuola".to_string(),
                    "Questa mattina devi tornare a scuola...".to_string(),
                ));
                state.studio = 0;
                for m in state.materie.iter_mut().skip(1) {
                    *m = 0;
                }
            }
        }
        12 => {
            if state.giorno > 22 {
                state.vacanza = 1;
                if state.rapporti > 0 {
                    state.current_tipa = 1;
                }
            }
            if state.giorno == 25 && state.current_pantaloni == 19 && state.current_gibbotto == 19 {
                rep.messages.push((
                    "Natale...".to_string(),
                    "Con il tuo vestito da Babbo Natale riesci a stupire tutti...".to_string(),
                ));
                state.fama += 20;
                if state.fama > 100 {
                    state.fama = 100;
                }
            }
            if state.giorno == 28 && (state.current_pantaloni == 19 || state.current_gibbotto == 19) {
                rep.messages.push((
                    "Natale...".to_string(),
                    "Natale e' gia' passato... Togliti quel dannato vestito...".to_string(),
                ));
                state.fama -= 5;
                if state.fama < 0 {
                    state.fama = 0;
                }
            }
        }
        _ => {}
    }

    if state.giornoset == 7 {
        state.vacanza = 2; // Domenica
    }

    // ---- Fixed holidays -------------------------------------------------------
    for (nome, g, m, descrizione) in VACANZE {
        if m == state.mese && g == state.giorno {
            rep.messages.push((nome.to_string(), descrizione.to_string()));
            state.vacanza = 2;
        }
    }

    rep
}

/// MostraPagella logic: k counts insufficient marks twice, promotion if k <= 4.
fn pagella(state: &GameState) -> Pagella {
    let mut k = 0;
    let mut voti = [0i32; 9];
    for i in 1..10 {
        if state.materie[i] < 6 {
            k += 1;
        }
        if state.materie[i] < 4 {
            k += 1;
        }
        voti[i - 1] = state.materie[i];
    }
    let condotta = if state.fama > 75 { 8 } else { 9 };
    let ammesso = k <= 4;
    // The +200 promotion bonus is applied by the caller when the pagella
    // is shown (Soldi += 200 inside MostraPagella in the original).
    Pagella { voti, condotta, ammesso }
}

/// Applies the promotion bonus (Soldi += 200 in MostraPagella); the event
/// engine calls this when the pagella has been shown.
pub fn bonus_promozione(state: &mut GameState) {
    state.soldi += 200;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn state() -> GameState {
        GameState::reset(true, &mut Rng::with_seed(1))
    }

    #[test]
    fn month_rollover_matches_the_original_start() {
        // The game starts on 30 September; the next day is 1 October.
        let mut s = state();
        giorno(&mut s);
        assert_eq!((s.giorno, s.mese, s.giornoset), (1, 10, 2));
    }

    #[test]
    fn february_29_in_leap_year_shows_the_message_once() {
        let mut s = state();
        s.mese = 2;
        s.giorno = 28;
        s.anno_bisesto = 1;
        let rep = giorno(&mut s);
        assert_eq!((s.giorno, s.mese), (29, 2));
        assert_eq!(rep.messages[0].0, "Anno Bisestile");
        // The day after, the month rolls over (30 > 28, not == 29).
        giorno(&mut s);
        assert_eq!((s.giorno, s.mese), (1, 3));
    }

    #[test]
    fn year_rollover_sets_bisesto_like_the_original() {
        let mut s = state();
        s.mese = 12;
        s.giorno = 31;
        giorno(&mut s);
        assert_eq!((s.giorno, s.mese), (1, 1));
        assert_eq!(s.anno_bisesto, 1); // the `= +1` quirk of the original
    }

    #[test]
    fn tan_fades_every_seven_days() {
        let mut s = state();
        s.current_testa = 3;
        for _ in 0..7 {
            giorno(&mut s); // giornoset cycles 2..7, 1
        }
        assert_eq!(s.current_testa, 2);
        assert_eq!(s.giornoset, 1);
    }

    #[test]
    fn sunday_is_type_two_holiday() {
        let mut s = state();
        s.giornoset = 6; // tomorrow is Sunday
        giorno(&mut s);
        assert_eq!(s.giornoset, 7);
        assert_eq!(s.vacanza, 2);
    }

    #[test]
    fn school_holidays_and_summer() {
        let mut s = state();
        s.mese = 1;
        s.giorno = 5;
        let rep = giorno(&mut s); // Jan 6: Epifania -> type 2 holiday
        assert_eq!(s.vacanza, 2);
        assert!(rep.messages.iter().any(|(t, _)| t == "Epifania"));

        let mut s = state();
        s.mese = 7;
        s.giorno = 10;
        giorno(&mut s);
        assert_eq!(s.vacanza, 1);

        let mut s = state();
        s.mese = 9;
        s.giorno = 10;
        giorno(&mut s);
        assert_eq!(s.vacanza, 1);
    }

    fn rep_has(rep: &GiornoReport, title: &str) -> bool {
        rep.messages.iter().any(|(t, _)| t == title)
    }

    fn giorno_of(s: &mut GameState, mese: i32, giorno_num: i32) -> GiornoReport {
        s.mese = mese;
        s.giorno = giorno_num;
        giorno(s)
    }

    #[test]
    fn first_school_day_resets_marks() {
        let mut s = state();
        s.materie[1] = 7;
        s.studio = 70;
        let rep = giorno_of(&mut s, 9, 14);
        assert!(rep_has(&rep, "Primo giorno di scuola"));
        assert_eq!(s.materie.iter().sum::<i32>(), 0);
        assert_eq!(s.studio, 0);
    }

    #[test]
    fn pagella_on_june_22() {
        let mut s = state();
        for m in s.materie.iter_mut().skip(1) {
            *m = 7; // all sufficient
        }
        s.calcola_studio();
        let rep = giorno_of(&mut s, 6, 21);
        let p = rep.pagella.expect("pagella expected on June 22");
        assert!(p.ammesso);
        assert_eq!(p.condotta, 9); // fama == 0
        assert_eq!(p.testo_ammesso('o'), "ammesso");

        // Failing marks -> NON ammesso.
        let mut s = state();
        for (i, m) in s.materie.iter_mut().enumerate().skip(1) {
            *m = if i % 2 == 0 { 2 } else { 10 }; // 4 gravinsuff + 1 insufficient -> k = 5
        }
        let rep = giorno_of(&mut s, 6, 21);
        let p = rep.pagella.unwrap();
        assert!(!p.ammesso);
        assert_eq!(p.testo_ammesso('o'), "NON ammesso");
    }

    #[test]
    fn santa_claus_suit_bonus_and_penalty() {
        let mut s = state();
        s.current_pantaloni = 19;
        s.current_gibbotto = 19;
        let rep = giorno_of(&mut s, 12, 24);
        assert_eq!(s.fama, 20);
        assert!(rep_has(&rep, "Natale..."));

        let mut s = state();
        s.current_gibbotto = 19;
        s.fama = 10;
        giorno_of(&mut s, 12, 27);
        assert_eq!(s.fama, 5);
    }

    #[test]
    fn stipendio_arrives_on_the_27th() {
        let mut s = state();
        s.impegno = 5;
        s.stipendio = 300;
        s.giorni_di_lavoro = 10;
        s.mese = 10;
        s.giorno = 26;
        let rep = giorno(&mut s);
        // giorni_di_lavoro becomes 11 before the check: 300 * 11 / 30 = 110.
        assert_eq!(s.soldi, 10 + 110);
        assert!(rep.messages.iter().any(|(t, _)| t == "Stipendio !"));
        assert_eq!(s.giorni_di_lavoro, 1);
    }

    #[test]
    fn palestra_expires() {
        let mut s = state();
        s.scad_pal_giorno = 15;
        s.scad_pal_mese = 10;
        let rep = giorno_of(&mut s, 10, 14);
        assert!(rep.messages.iter().any(|(t, _)| t == "Palestra"));
        assert_eq!((s.scad_pal_giorno, s.scad_pal_mese), (0, 0));
    }
}
