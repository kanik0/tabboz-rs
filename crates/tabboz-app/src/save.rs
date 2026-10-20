// Save/load: JSON snapshot of the game state in the per-user config dir:
//   macOS:   ~/Library/Application Support/tabboz/tabboz.json
//   Linux:   ~/.config/tabboz/tabboz.json
//   Windows: %APPDATA%\tabboz\tabboz.json
// (the original used the Windows registry / a save file; a single JSON
// slot is the cross-platform equivalent, with the same field names as
// SalvaTutto).

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use tabboz_core::state::{GameState, Scooter};
use tabboz_core::Rng;

fn save_path() -> Option<PathBuf> {
    // dirs::config_dir() returns the right per-user config directory on
    // every platform (macOS Application Support, Linux XDG, Windows Roaming).
    let base = dirs::config_dir()?.join("tabboz");
    std::fs::create_dir_all(&base).ok()?;
    Some(base.join("tabboz.json"))
}

#[derive(Serialize, Deserialize, Debug)]
struct SaveGame {
    // Money (thousands of lire).
    soldi: u64,
    paghetta: u64,
    ddp: u64,
    reputazione: i32,
    fama: i32,
    studio: i32,
    stato: i32,
    fortuna: i32,
    attesa: i32,
    sesso: char,
    rapporti: i32,
    fig_tipa: i32,
    nometipa: String,
    impegno: i32,
    giorni_di_lavoro: i32,
    stipendio: i64,
    numeroditta: i32,
    sizze: i32,
    tempo_pestaggio: i32,
    materie: [i32; 10],
    scooter: SaveScooter,
    cellular: SaveCellular,
    abbonamento: SaveAbbonamento,
    benzina: i32,
    antifurto: i32,
    giorno: i32,
    mese: i32,
    giornoset: i32,
    anno_bisesto: i32,
    scad_pal_giorno: i32,
    scad_pal_mese: i32,
    comp_giorno: i32,
    comp_mese: i32,
    current_testa: i32,
    current_gibbotto: i32,
    current_pantaloni: i32,
    current_scarpe: i32,
    nome: String,
    cognome: String,
    city: String,
    residenza: String,
    street: String,
    euro: bool,
    sound_active: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct SaveScooter {
    speed: i32,
    marmitta: i32,
    carburatore: i32,
    cc: i32,
    filtro: i32,
    prezzo: i64,
    attivita: i32,
    stato: i32,
    nome: String,
    fama: i32,
}

#[derive(Serialize, Deserialize, Debug)]
struct SaveCellular {
    dual: i32,
    fama: i32,
    stato: i32,
    prezzo: i64,
    nome: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct SaveAbbonamento {
    abbonamento: i32,
    dualonly: i32,
    creditorest: i32,
    fama: i32,
    prezzo: i64,
    nome: String,
}

/// Serializes the whole game state to the autosave slot.
pub fn save(state: &GameState) {
    let Some(path) = save_path() else { return };
    let s = SaveGame {
        soldi: state.soldi,
        paghetta: state.paghetta,
        ddp: state.ddp,
        reputazione: state.reputazione,
        fama: state.fama,
        studio: state.studio,
        stato: state.stato,
        fortuna: state.fortuna,
        attesa: state.attesa,
        sesso: state.sesso,
        rapporti: state.rapporti,
        fig_tipa: state.fig_tipa,
        nometipa: state.nometipa.clone(),
        impegno: state.impegno,
        giorni_di_lavoro: state.giorni_di_lavoro,
        stipendio: state.stipendio,
        numeroditta: state.numeroditta,
        sizze: state.sizze,
        tempo_pestaggio: state.tempo_pestaggio,
        materie: state.materie,
        scooter: SaveScooter {
            speed: state.scooter.speed,
            marmitta: state.scooter.marmitta,
            carburatore: state.scooter.carburatore,
            cc: state.scooter.cc,
            filtro: state.scooter.filtro,
            prezzo: state.scooter.prezzo,
            attivita: state.scooter.attivita,
            stato: state.scooter.stato,
            nome: state.scooter.nome.clone(),
            fama: state.scooter.fama,
        },
        cellular: SaveCellular {
            dual: state.cellular.dual,
            fama: state.cellular.fama,
            stato: state.cellular.stato,
            prezzo: state.cellular.prezzo,
            nome: state.cellular.nome.clone(),
        },
        abbonamento: SaveAbbonamento {
            abbonamento: state.abbonamento.abbonamento,
            dualonly: state.abbonamento.dualonly,
            creditorest: state.abbonamento.creditorest,
            fama: state.abbonamento.fama,
            prezzo: state.abbonamento.prezzo,
            nome: state.abbonamento.nome.clone(),
        },
        benzina: state.benzina,
        antifurto: state.antifurto,
        giorno: state.giorno,
        mese: state.mese,
        giornoset: state.giornoset,
        anno_bisesto: state.anno_bisesto,
        scad_pal_giorno: state.scad_pal_giorno,
        scad_pal_mese: state.scad_pal_mese,
        comp_giorno: state.comp_giorno,
        comp_mese: state.comp_mese,
        current_testa: state.current_testa,
        current_gibbotto: state.current_gibbotto,
        current_pantaloni: state.current_pantaloni,
        current_scarpe: state.current_scarpe,
        nome: state.nome.clone(),
        cognome: state.cognome.clone(),
        city: state.city.clone(),
        residenza: state.residenza.clone(),
        street: state.street.clone(),
        euro: state.euro,
        sound_active: state.sound_active,
    };
    // Compact JSON: the save file is machine-read, pretty-printing only
    // bloats it and slows down serialization.
    if let Ok(json) = serde_json::to_string(&s) {
        if let Ok(mut f) = std::fs::File::create(&path) {
            let _ = f.write_all(json.as_bytes());
        }
    }
}

/// Loads the autosave slot, if it exists and is valid.
pub fn load() -> Option<GameState> {
    let path = save_path()?;
    let json = std::fs::read_to_string(path).ok()?;
    let s: SaveGame = serde_json::from_str(&json).ok()?;
    let mut state = GameState::reset(false, &mut Rng::with_seed(0));
    state.soldi = s.soldi;
    state.paghetta = s.paghetta;
    state.ddp = s.ddp;
    state.reputazione = s.reputazione;
    state.fama = s.fama;
    state.studio = s.studio;
    state.stato = s.stato;
    state.fortuna = s.fortuna;
    state.attesa = s.attesa;
    state.sesso = s.sesso;
    state.rapporti = s.rapporti;
    state.fig_tipa = s.fig_tipa;
    state.nometipa = s.nometipa;
    state.impegno = s.impegno;
    state.giorni_di_lavoro = s.giorni_di_lavoro;
    state.stipendio = s.stipendio;
    state.numeroditta = s.numeroditta;
    state.sizze = s.sizze;
    state.tempo_pestaggio = s.tempo_pestaggio;
    state.materie = s.materie;
    state.scooter = Scooter {
        speed: s.scooter.speed,
        marmitta: s.scooter.marmitta,
        carburatore: s.scooter.carburatore,
        cc: s.scooter.cc,
        filtro: s.scooter.filtro,
        prezzo: s.scooter.prezzo,
        attivita: s.scooter.attivita,
        stato: s.scooter.stato,
        nome: s.scooter.nome,
        fama: s.scooter.fama,
    };
    state.cellular.dual = s.cellular.dual;
    state.cellular.fama = s.cellular.fama;
    state.cellular.stato = s.cellular.stato;
    state.cellular.prezzo = s.cellular.prezzo;
    state.cellular.nome = s.cellular.nome;
    state.abbonamento.abbonamento = s.abbonamento.abbonamento;
    state.abbonamento.dualonly = s.abbonamento.dualonly;
    state.abbonamento.creditorest = s.abbonamento.creditorest;
    state.abbonamento.fama = s.abbonamento.fama;
    state.abbonamento.prezzo = s.abbonamento.prezzo;
    state.abbonamento.nome = s.abbonamento.nome;
    state.benzina = s.benzina;
    state.antifurto = s.antifurto;
    state.giorno = s.giorno;
    state.mese = s.mese;
    state.giornoset = s.giornoset;
    state.anno_bisesto = s.anno_bisesto;
    state.scad_pal_giorno = s.scad_pal_giorno;
    state.scad_pal_mese = s.scad_pal_mese;
    state.comp_giorno = s.comp_giorno;
    state.comp_mese = s.comp_mese;
    state.current_testa = s.current_testa;
    state.current_gibbotto = s.current_gibbotto;
    state.current_pantaloni = s.current_pantaloni;
    state.current_scarpe = s.current_scarpe;
    state.nome = s.nome;
    state.cognome = s.cognome;
    state.city = s.city;
    state.residenza = s.residenza;
    state.street = s.street;
    state.euro = s.euro;
    state.sound_active = s.sound_active;
    Some(state)
}
