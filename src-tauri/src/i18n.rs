//! Interface language, decided once at startup from the Windows display language.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Locale {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "pt-BR")]
    PtBr,
}

/// Primary language of a Windows `LANGID`, its low 10 bits.
const LANG_PORTUGUESE: u16 = 0x16;

impl Locale {
    /// Any Portuguese gets pt-BR, the only Portuguese available; everything else gets English.
    pub fn from_langid(langid: u16) -> Self {
        if langid & 0x3ff == LANG_PORTUGUESE {
            Self::PtBr
        } else {
            Self::En
        }
    }

    pub fn current() -> Self {
        Self::from_langid(crate::platform::ui_language())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portuguese_gets_pt_br_and_the_rest_english() {
        assert_eq!(Locale::from_langid(0x0416), Locale::PtBr); // pt-BR
        assert_eq!(Locale::from_langid(0x0816), Locale::PtBr); // pt-PT
        assert_eq!(Locale::from_langid(0x0409), Locale::En); // en-US
        assert_eq!(Locale::from_langid(0x0c0a), Locale::En); // es-ES
        assert_eq!(Locale::from_langid(0), Locale::En);
    }
}
