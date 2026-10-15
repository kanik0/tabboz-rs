// MostraSoldi: money formatting of the original game.
//
// Money is stored in THOUSANDS of lire (u_long in the original):
//   MostraSoldi(10) -> "10000 L."   MostraSoldi(0) -> "0 L."
// In euro mode the value is halved and suffixed with "e".

/// Formats an amount (in thousands of lire) exactly like the original.
pub fn mostra_soldi(migliaia: u64, euro: bool) -> String {
    if euro {
        format!("{}e", migliaia / 2)
    } else if migliaia == 0 {
        "0 L.".to_string()
    } else {
        format!("{}000 L.", migliaia)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_lire_and_euro() {
        assert_eq!(mostra_soldi(0, false), "0 L.");
        assert_eq!(mostra_soldi(10, false), "10000 L.");
        assert_eq!(mostra_soldi(123_456, false), "123456000 L.");
        assert_eq!(mostra_soldi(10, true), "5e");
        assert_eq!(mostra_soldi(11, true), "5e"); // integer division
    }
}
