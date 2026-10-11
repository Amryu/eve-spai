/// EDENCOM fortress systems, where the strongest EDENCOM fleets and heavy gunstars stay since the
/// Triglavian invasions ended. Static, baked in from the kybernaut.space invasion list (2020);
/// ids are sorted for `binary_search`.
pub static FORTRESS_IDS: &[i64] = &[
    30000004, // Jark
    30000005, // Sasta
    30000105, // Abha
    30000113, // Astabih
    30000118, // Uanzin
    30000188, // Hentogaira
    30002242, // Mamenkhanar
    30002243, // Seiradih
    30002251, // Sadye
    30002253, // Arshat
    30002266, // Ahmak
    30002385, // Teonusude
    30002386, // Gelfiven
    30002530, // Avesber
    30002651, // Fasse
    30002662, // Pulin
    30002665, // Misneden
    30002700, // Bawilan
    30002704, // Adrallezoen
    30002986, // Mendori
    30003050, // Odixie
    30003392, // Eygfe
    30003397, // Bongveber
    30003398, // Anbald
    30003490, // Khopa
    30003514, // Yeeramoun
    30003515, // Anila
    30003539, // Miakie
    30003541, // Faswiba
    30003548, // Barira
    30003553, // Warouh
    30003556, // Arton
    30003573, // Pertnineere
    30003574, // Boystin
    30003883, // Keberz
    30003885, // Arzanni
    30004084, // Ghesis
    30004090, // Aband
    30004100, // Halibai
    30004103, // Kothe
    30004141, // Hiremir
    30004150, // Shaggoth
    30004248, // Haimeh
    30004250, // Chibi
    30004305, // Esaeel
    30004973, // Caslemon
    30004992, // Palmon
    30005052, // Soumi
    30005058, // Neesher
    30005251, // Asanot
    30005252, // Anzalaisio
    30005260, // Keri
    30045322, // Samanuni
];

/// EDENCOM minor victory systems, with weaker fleets and gunstars than a fortress.
pub static MINOR_VICTORY_IDS: &[i64] = &[
    30000012, // Asabona
    30000048, // Ihal
    30000060, // Janus
    30000062, // Iosantin
    30000102, // Dysa
    30000109, // Berta
    30000160, // Reisen
    30001376, // Nourvukaiken
    30001660, // Dabrid
    30001696, // Iro
    30001718, // Paye
    30002048, // Bei
    30002051, // Anher
    30002239, // Rammi
    30002241, // Rimbah
    30002397, // Horaka
    30002506, // Osoggur
    30002513, // Dammalin
    30002644, // Ambeke
    30002724, // Assiettes
    30002755, // Usi
    30002772, // Rairomon
    30002999, // Shastal
    30003058, // Olide
    30003061, // Mormelot
    30003074, // Sasiekko
    30003078, // Erkinen
    30003088, // Oyonata
    30003090, // Saidusairos
    30003460, // Offikatlin
    30003463, // Erlendur
    30003478, // Basan
    30003480, // Amod
    30003481, // Unefsih
    30003482, // Mista
    30003558, // Madimal
    30003570, // Elore
    30003587, // Harner
    30003788, // Intaki
    30003794, // Stacmon
    30003809, // Brellystier
    30003823, // Kenninck
    30003824, // Archavoinet
    30003829, // Renarelle
    30003854, // Alamel
    30003894, // Sabusi
    30003900, // Ham
    30003904, // Col
    30003908, // Bashyam
    30003918, // Hakana
    30003919, // Ashkoo
    30003927, // Zahefeus
    30003931, // Sassecho
    30003932, // Timudan
    30004108, // Chaneya
    30004231, // Shakasi
    30004254, // Fihrneh
    30004256, // Edilkam
    30004257, // Hakatiz
    30004263, // Feshur
    30004268, // Shenda
    30004284, // Defsunun
    30004287, // Esubara
    30004289, // Vaini
    30004295, // Keba
    30004301, // Anath
    30004302, // Omigiav
    30004978, // Pemene
    30004999, // Ladistier
    30005034, // Bridi
    30005066, // Kerying
    30005074, // Daran
    30005086, // Arza
    30005209, // Sibe
    30005213, // Hesarid
    30005219, // Sigga
    30005222, // Serren
    30005236, // Noranim
    30005255, // Saphthar
    30005263, // Mozzidit
    30005267, // Bherdasopt
    30005284, // Promised Land
    30005308, // Jufvitte
    30005334, // Tierijev
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Fortress,
    MinorVictory,
}

pub fn kind(id: i64) -> Option<Kind> {
    if FORTRESS_IDS.binary_search(&id).is_ok() {
        Some(Kind::Fortress)
    } else if MINOR_VICTORY_IDS.binary_search(&id).is_ok() {
        Some(Kind::MinorVictory)
    } else {
        None
    }
}

pub fn has(id: i64) -> bool {
    kind(id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_are_sorted_and_disjoint() {
        for list in [FORTRESS_IDS, MINOR_VICTORY_IDS] {
            assert!(list.windows(2).all(|w| w[0] < w[1]));
        }
        assert!(FORTRESS_IDS.iter().all(|id| !MINOR_VICTORY_IDS.contains(id)));
        assert_eq!((FORTRESS_IDS.len(), MINOR_VICTORY_IDS.len()), (53, 84));
    }

    #[test]
    fn kinds() {
        assert_eq!(kind(30003539), Some(Kind::Fortress));
        assert_eq!(kind(30003088), Some(Kind::MinorVictory));
        assert_eq!(kind(30004759), None);
    }
}
