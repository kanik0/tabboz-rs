// Game state: the Rust form of all the globals of zarrosim.c/zarrosim.h,
// plus ResetMe (initial state) and the small shared helpers
// (vvc, CalcolaStudio, CalcolaSesso).

use crate::rng::Rng;

/// School subjects (scuola.c MaterieMem). Index 0 is unused, like the original.
pub const MATERIE: [&str; 10] = [
    "---",
    "Agraria",
    "Fisica",
    "Attivita' culturali",
    "Attivita' matematiche",
    "Scienze industriali",
    "Elettrochimica",
    "Petrolchimica",
    "Filosofia aziendale",
    "Metallurgia",
];

/// NEWSTSCOOTER: the scooter the tabbozzo currently owns.
#[derive(Clone, Debug, PartialEq)]
pub struct Scooter {
    pub speed: i32,
    pub marmitta: i32,    // +0, +7, +12, +15
    pub carburatore: i32, // 0 - 4
    pub cc: i32,          // 0 - 4
    pub filtro: i32,      // +0, +5, +10, +15
    pub prezzo: i64,
    pub attivita: i32,
    /// -1 = no scooter.
    pub stato: i32,
    pub nome: String,
    pub fama: i32,
}

impl Default for Scooter {
    fn default() -> Self {
        Scooter::none()
    }
}

impl Scooter {
    /// ScooterMem[0]: "nessuno scooter".
    pub fn none() -> Self {
        Scooter {
            speed: 0,
            marmitta: 0,
            carburatore: 0,
            cc: 0,
            filtro: 0,
            prezzo: 0,
            attivita: 0,
            stato: -1,
            nome: String::new(),
            fama: 0,
        }
    }
}

/// STCEL: the cellular phone.
#[derive(Clone, Debug, Default)]
pub struct Cellular {
    pub dual: i32,
    pub fama: i32,
    /// -1 = no phone, 0 = dying, 1 = ok.
    pub stato: i32,
    pub prezzo: i64,
    pub nome: String,
}

/// STABB: phone subscription.
#[derive(Clone, Debug, Default)]
pub struct Abbonamento {
    pub abbonamento: i32, // 0 = ricarica, 1 = abbonamento
    pub dualonly: i32,
    pub creditorest: i32,
    pub fama: i32,
    pub prezzo: i64,
    pub nome: String,
}

/// All the mutable state of a game session.
#[derive(Clone, Debug)]
pub struct GameState {
    // Money (thousands of lire).
    pub soldi: u64,
    pub paghetta: u64,
    /// Due di picche counter ("lo zarro è ottimista").
    pub ddp: u64,

    // Character.
    pub reputazione: i32,
    pub fama: i32,
    pub studio: i32,
    pub stato: i32, // health/mood 0-100
    pub fortuna: i32,
    pub attesa: i32,
    pub sesso: char,

    // Tipa.
    pub rapporti: i32,
    pub fig_tipa: i32,
    pub nometipa: String,

    // Work.
    pub impegno: i32,
    pub giorni_di_lavoro: i32,
    pub stipendio: i64,
    pub numeroditta: i32,

    // Habits.
    pub sizze: i32,
    pub tempo_pestaggio: i32,

    // School (index 0 unused).
    pub materie: [i32; 10],

    // Belongings.
    pub scooter: Scooter,
    pub cellular: Cellular,
    pub abbonamento: Abbonamento,
    pub benzina: i32,
    pub antifurto: i32,

    // Calendar.
    pub giorno: i32,
    pub mese: i32,
    pub giornoset: i32, // 1..=7, 7 = Sunday
    pub anno_bisesto: i32,
    pub vacanza: i32, // 0 no, 1 school holidays, 2 also shops closed
    pub scad_pal_giorno: i32,
    pub scad_pal_mese: i32,
    pub comp_giorno: i32,
    pub comp_mese: i32,

    // Look (indexes into the bitmap resources).
    pub current_testa: i32,
    pub current_gibbotto: i32,
    pub current_pantaloni: i32,
    pub current_scarpe: i32,
    pub current_tipa: i32,

    // Identity.
    pub nome: String,
    pub cognome: String,
    pub city: String,
    pub residenza: String,
    pub street: String,
    pub euro: bool,
    pub sound_active: bool,
}

impl GameState {
    /// ResetMe(primavolta) of zarrosim.c.
    /// ResetMe(primavolta): in the original primavolta also resets the
    /// configuration; here reset() always produces the fresh defaults
    /// (sesso M, Tizio Caio, sound on) so the flag needs no branch.
    pub fn reset(_primavolta: bool, rng: &mut Rng) -> Self {
        let city = string((400 + rng.random(22)) as u16);
        let via = string((450 + rng.random(50)) as u16);
        let street = format!("{} n. {}", via, 1 + rng.random(150));

        let mut state = GameState {
            soldi: 10,
            paghetta: 30,
            ddp: 0,
            reputazione: 0,
            fama: 0,
            studio: 0,
            stato: 100,
            fortuna: 0,
            attesa: crate::ATTESA_MAX,
            sesso: 'M',
            rapporti: 0,
            fig_tipa: 0,
            nometipa: String::new(),
            impegno: 0,
            giorni_di_lavoro: 0,
            stipendio: 0,
            numeroditta: 0,
            sizze: 0,
            tempo_pestaggio: 0,
            materie: [0; 10],
            scooter: Scooter::none(),
            cellular: Cellular { stato: -1, ..Default::default() },
            abbonamento: Abbonamento { creditorest: -1, ..Default::default() },
            benzina: 0,
            antifurto: 0,
            giorno: 30,
            mese: 9,
            giornoset: 1,
            anno_bisesto: 0,
            vacanza: 0,
            scad_pal_giorno: 0,
            scad_pal_mese: 0,
            comp_giorno: 0,
            comp_mese: 0,
            current_testa: 0,
            current_gibbotto: 0,
            current_pantaloni: 0,
            current_scarpe: 0,
            current_tipa: 0,
            nome: "Tizio".to_string(),
            cognome: "Caio".to_string(),
            city,
            residenza: "Milano".to_string(),
            street,
            euro: false,
            sound_active: true,
        };

        // Compleanno random (giorno within the month, 1..=12 for the month).
        state.comp_mese = rng.random(12) + 1;
        state.comp_giorno = rng.random(crate::calendar::giorni_mese(state.comp_mese)) + 1;

        if state.sesso == 'F' {
            state.nome = "Nessuna".to_string();
            state.cognome = "In Particolare".to_string();
        }

        state.calcola_studio();
        state
    }

    /// vvc(): clamp a loaded value into 0..=100.
    pub fn vvc(i: i32) -> i32 {
        i.clamp(0, 100)
    }

    /// CalcolaSesso(): derived gender forms.
    pub fn ao(&self) -> char {
        if self.sesso == 'M' { 'o' } else { 'a' }
    }

    pub fn un_una(&self) -> &'static str {
        if self.sesso == 'M' { "un" } else { "una" }
    }

    /// CalcolaStudio(): Studio = (sum of the 9 subject marks * 10) / 9.
    pub fn calcola_studio(&mut self) {
        let sum: i32 = self.materie[1..10].iter().sum();
        self.studio = sum * 10 / 9;
    }
}

/// Loads a string table entry (tabboz-res, generated from the original
/// STRINGTABLE resources); panics loudly if the port is missing data.
pub fn string(id: u16) -> String {
    tabboz_res::strings::find(id)
        .unwrap_or_else(|| panic!("missing string table entry {id}"))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_defaults_match_the_original() {
        let mut rng = Rng::with_seed(1);
        let s = GameState::reset(true, &mut rng);
        assert_eq!(s.soldi, 10);
        assert_eq!(s.paghetta, 30);
        assert_eq!(s.stato, 100);
        assert_eq!(s.scooter.stato, -1);
        assert_eq!(s.cellular.stato, -1);
        assert_eq!(s.abbonamento.creditorest, -1);
        assert_eq!((s.giorno, s.mese, s.giornoset), (30, 9, 1));
        assert_eq!(s.residenza, "Milano");
        assert_eq!(s.nome, "Tizio");
        assert_eq!(s.cognome, "Caio");
        assert!(!s.street.is_empty());
        assert!((1..=12).contains(&s.comp_mese));
        assert!(s.comp_giorno >= 1 && s.comp_giorno <= 31);
    }

    #[test]
    fn female_identity() {
        let mut rng = Rng::with_seed(2);
        let mut s = GameState::reset(false, &mut rng);
        s.sesso = 'F';
        // ResetMe with sesso == 'F' overwrites the identity.
        let s = GameState::reset(false, &mut { Rng::with_seed(2) });
        let mut s = s;
        s.sesso = 'F';
        assert_eq!(s.ao(), 'a');
        assert_eq!(s.un_una(), "una");
    }

    #[test]
    fn vvc_clamps() {
        assert_eq!(GameState::vvc(-5), 0);
        assert_eq!(GameState::vvc(50), 50);
        assert_eq!(GameState::vvc(101), 100);
    }

    #[test]
    fn calcola_studio_matches_the_original_formula() {
        let mut s = GameState::reset(true, &mut Rng::with_seed(3));
        for m in s.materie.iter_mut().skip(1) {
            *m = 10;
        }
        s.calcola_studio();
        assert_eq!(s.studio, 100); // 9 * 10 * 10 / 9

        s.materie = [0, 10, 9, 8, 7, 6, 5, 4, 3, 2];
        s.calcola_studio();
        assert_eq!(s.studio, 54 * 10 / 9); // 60
        assert_eq!(s.studio, 60);
    }

    #[test]
    fn all_strings_used_by_the_core_exist() {
        // Cities (400 + random(22)), streets (450 + random(50)).
        for id in 400..422 {
            assert!(tabboz_res::strings::find(id).is_some(), "missing string {id}");
        }
        for id in 450..500 {
            assert!(tabboz_res::strings::find(id).is_some(), "missing string {id}");
        }
        // Female names (200 + random(20)) and male names (1200 + random(20)).
        for id in 200..220 {
            assert!(tabboz_res::strings::find(id).is_some(), "missing string {id}");
        }
        for id in 1200..1220 {
            assert!(tabboz_res::strings::find(id).is_some(), "missing string {id}");
        }
        // Event texts 1000..1001 and 1021..1050, dumping texts 1341..1350.
        for id in 1000..=1001 {
            assert!(tabboz_res::strings::find(id).is_some(), "missing string {id}");
        }
        for id in 1021..=1050 {
            assert!(tabboz_res::strings::find(id).is_some(), "missing string {id}");
        }
        for id in 1341..=1350 {
            assert!(tabboz_res::strings::find(id).is_some(), "missing string {id}");
        }
    }
}
