//! The Windows spell checkers for the user's languages (Settings > Time & language > Language).
//! COM objects stay on the thread that created them: see `crate::spelling`.
use windows::System::UserProfile::GlobalizationPreferences;
use windows::Win32::Foundation::S_OK;
use windows::Win32::Globalization::{ISpellChecker, ISpellCheckerFactory, SpellCheckerFactory};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::core::{HSTRING, PWSTR};

pub struct Checkers(Vec<(String, ISpellChecker)>);

impl Checkers {
    /// Joins this thread to COM and opens one checker per user language with a dictionary, in the
    /// user's order. Without any, every word reads as correct.
    pub fn open() -> Self {
        let mut checkers = Vec::new();
        if let Err(error) = unsafe { Self::create(&mut checkers) } {
            eprintln!("[spelling] {error}");
        }
        Self(checkers)
    }

    unsafe fn create(checkers: &mut Vec<(String, ISpellChecker)>) -> windows::core::Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let factory: ISpellCheckerFactory =
                CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER)?;
            let languages = GlobalizationPreferences::Languages()?;
            for index in 0..languages.Size()? {
                let tag = languages.GetAt(index)?;
                let name = tag.to_string();
                if checkers.iter().any(|(known, _)| *known == name)
                    || !factory.IsSupported(&tag)?.as_bool()
                {
                    continue;
                }
                match factory.CreateSpellChecker(&tag) {
                    Ok(checker) => checkers.push((name, checker)),
                    Err(error) => eprintln!("[spelling] {tag}: {error}"),
                }
            }
        }
        Ok(())
    }

    /// BCP 47 tags, such as `pt-BR`, in the user's order.
    pub fn languages(&self) -> Vec<String> {
        self.0.iter().map(|(tag, _)| tag.clone()).collect()
    }

    /// Whether one checker flags the word; a failure reads as correct.
    pub fn flags(&self, index: usize, word: &str) -> bool {
        let Some((_, checker)) = self.0.get(index) else {
            return false;
        };
        unsafe {
            let Ok(errors) = checker.Check(&HSTRING::from(word)) else {
                return false;
            };
            let mut error = None;
            errors.Next(&mut error) == S_OK && error.is_some()
        }
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }

    /// One checker's suggestions, best first.
    pub fn suggest(&self, index: usize, word: &str) -> Vec<String> {
        let mut found = Vec::new();
        let Some((_, checker)) = self.0.get(index) else {
            return found;
        };
        unsafe {
            let Ok(list) = checker.Suggest(&HSTRING::from(word)) else {
                return found;
            };
            loop {
                let mut item = [PWSTR::null()];
                let mut fetched = 0;
                if list.Next(&mut item, Some(&mut fetched)) != S_OK || fetched == 0 {
                    break;
                }
                if let Ok(text) = item[0].to_string() {
                    found.push(text);
                }
                CoTaskMemFree(Some(item[0].0 as _));
            }
        }
        found
    }

    /// Adds the word to the first language's user dictionary, shared with other Windows apps.
    pub fn add(&self, word: &str) -> windows::core::Result<()> {
        match self.0.first() {
            Some((_, checker)) => unsafe { checker.Add(&HSTRING::from(word)) },
            None => Ok(()),
        }
    }
}
